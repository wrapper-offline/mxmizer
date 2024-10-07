use as3_parser::ns::*;
use if_chain::if_chain;
use std::borrow::Cow;
use std::error::Error;
use std::fs;
use std::io;
use xml::attribute::Attribute;
use xml::name::Name;
use xml::namespace::Namespace;
use xml::writer::{EmitterConfig, XmlEvent};

const BLOCK_PROPS:[&str; 4] = ["_bindings", "_watchers", "_bindingsByDestination", "_bindingsBeginWithWord"];
const DEFAULT_NAMESPACES:&[DefaultNamespace] = &[
	DefaultNamespace {
		base_package: &"fx",
		name: &"fx",
		value: &"http://ns.adobe.com/mxml/2009",
	},
	DefaultNamespace {
		base_package: &"spark",
		name: &"s",
		value: &"library://ns.adobe.com/flex/spark",
	},
	DefaultNamespace {
		base_package: "mx",
		name: &"mx",
		value: &"library://ns.adobe.com/flex/mx",
	},
];

struct DefaultNamespace {
	base_package: &'static str,
	name: &'static str,
	value: &'static str,
}

/// stringifies an expression and returns it
fn expr_to_string(expr:&Expression) -> String {
	match expr {
		Expression::StringLiteral(literal) =>
			return literal.value.to_owned(),
		Expression::NumericLiteral(literal) =>
			return literal.value.to_owned(),
		Expression::BooleanLiteral(literal) =>
			return literal.value.to_owned().to_string(),
		_ =>return "".into()
	}
}

#[derive(Debug, PartialEq)]
struct ElemAttribute(String, String);
impl ElemAttribute {
	fn parse(mut name:String, val_expr:&Expression) -> Option<ElemAttribute> {
		if BLOCK_PROPS.contains(&name.as_str()) {
			return None;
		}
		let mut value = expr_to_string(val_expr);
		match name.as_str() {
			"percentWidth" => {
				name = "width".into();
				value += "%";
			},
			"percentHeight" => {
				name = "height".into();
				value += "%";
			},
			val => name = String::from(val.to_owned())
		}
		Some(ElemAttribute(name, value))
	}
}

#[derive(Debug, PartialEq)]
struct MxmlElement {
	namespace: String,
	class_name: String,
	attributes: Vec<ElemAttribute>,
	children: Vec<MxmlElement>
}
impl MxmlElement {
	fn get_parser_events(&self, doc:&MxmlDoc) -> Result<Vec<XmlEvent>, Box<dyn Error>> {
		let mut attributes = Vec::new();

		for attribute in &self.attributes {
			attributes.push(Attribute {
				name: Name {
					local_name: attribute.0.as_str(),
					namespace: None,
					prefix: None
				},
				value: attribute.1.as_str()
			});
		}

		// add namespaces if we're on the root element
		let namespace:Namespace;
		if doc.root.as_ref().unwrap() == self {
			namespace = doc.namespaces.to_owned();
		} else {
			namespace = Namespace::empty();
		}

		let start = XmlEvent::StartElement {
			name: Name {
				local_name: &self.class_name,
				namespace: None,
				prefix: Some(&self.namespace),
			},
			attributes: Cow::from(attributes),
			namespace: Cow::Owned(namespace),
		};

		let mut events:Vec<XmlEvent> = Vec::new();
		events.push(start.into());
		events.push(XmlEvent::end_element().into());
		Ok(events)
	}
}

struct Binding {
	dest_id: String,
	dest_attribute: String,
	source: String,
}

struct MxmlDoc {
	imports: Vec<ImportDirective>,
	class: ClassDefinition,
	root: Option<MxmlElement>,
	namespaces: Namespace,
	bindings: Vec<Binding>,
}
impl MxmlDoc {
	fn parse(content:&String) -> Option<MxmlDoc> {
		// parse file
		let compilation_unit = CompilationUnit::new(None, content.to_owned());
		let parser_options = ParserOptions::default();
		let program = ParserFacade(&compilation_unit, parser_options).parse_program();

		let mut imports = Vec::new();
		let package = program.packages.iter().next().unwrap();
		for directive in package.block.directives.iter() {
			match directive.as_ref() {
				Directive::ImportDirective(import) => {
					imports.push(import.to_owned());
				},
				Directive::ClassDefinition(class) => {
					let mut namespace = Namespace::empty();
					for def_ns in DEFAULT_NAMESPACES {
						namespace.put(def_ns.name, def_ns.value);
					}

					let mut doc = MxmlDoc {
						imports,
						class: class.to_owned(),
						root: None,
						namespaces: namespace,
						bindings: Vec::new(),
					};
					if !doc.is_valid_doc() {
						return None;
					}
					// store the bindings first so we can use them during element parsing
					// doc.bindings
		
					doc.root = doc.parse_constructor();
					return Some(doc);
				},
				_ => continue,
			}
		}

		None
	}

	fn get_function(&self, name:&String) -> Option<&FunctionDefinition> {
		for directive in self.class.block.directives.iter() {
			if_chain! {
				if let Directive::FunctionDefinition(defn) = directive.as_ref();
				if &defn.name_identifier().0 == name;
				then {
					return Some(defn);
				}
			}
		}
		None
	}

	/// returns the name of an imported package as a vector of strings
	fn get_package_name(&self, specifier:&String) -> Option<Vec<String>> {
		for import in self.imports.iter() {
			if_chain! {
				if let ImportSpecifier::Identifier(id) = &import.import_specifier;
				if &id.0 == specifier;
				then {
					let name = 
						import.package_name.to_owned()
						.iter_mut()
						.map(|v| return v.0.to_owned())
						.collect::<Vec<String>>();
					return Some(name.to_owned());
				}
			}
		}
		None
	}

	fn use_namespace(&mut self, specifier:&String) -> String {
		let full_name = self
			.get_package_name(specifier)
			.expect("Import should have existed");

		let first_val = full_name
			.first()
			.expect("Package name should have had at least one element");
		let def_ns = DEFAULT_NAMESPACES.into_iter().find(|ns| ns.base_package == first_val);
		if let Some(def_ns) = def_ns {
			return def_ns.name.into();
		}

		let name = full_name.last().unwrap().to_owned();
		let value = full_name.join(".") + ".*";
		self.namespaces.put(name.clone(), value);
		name
	}

	fn parse_constructor(&mut self) -> Option<MxmlElement> {
		let constructor = self.get_function(&self.class.name.0).unwrap();
		let body = constructor.common.body.as_ref().unwrap();
		let FunctionBody::Block(body) = body else {
			return None;
		};
		let mut attributes:Vec<ElemAttribute> = Vec::new();
		for expr in body.directives.iter() {
			let Directive::ExpressionStatement(expr) = expr.as_ref() else {
				continue;
			};
			match expr.expression.as_ref() {
				Expression::Assignment(expr) => {
					if_chain! {
						if let Expression::Member(left) = expr.left.as_ref();
						// only accept properties of `this`
						if let Expression::ThisLiteral(_) = left.base.as_ref();
						if let QualifiedIdentifierIdentifier::Id(id) = &left.identifier.id;
						then {
							// children array
							if id.0 == "mxmlContent" {
								//TODO PARSE CHILDREn
								continue;
							}
							// embeds
							if id.0.contains("_embed_mxml_") {
								// TOdo proper embed handler
								continue;
							}
							// attributes OR properties
							match ElemAttribute::parse(id.0.clone(), expr.right.as_ref()) {
								Some(attr) => attributes.push(attr),
								None => continue,
							}
							continue;
						}
					}
					// states
					if_chain! {
						if let Expression::QualifiedIdentifier(left) = expr.left.as_ref();
						if let QualifiedIdentifierIdentifier::Id(id) = &left.id;
						if id.0 != "states";
						then {
							// TODO PARE STAETS
						} else {
							continue;
						}
					}
				}
				// component declarations
				Expression::Call(expr) => {

				}
				_ => {}
			}
		}
		let Expression::QualifiedIdentifier(extends) = self.class.extends_clause.as_ref().unwrap().as_ref() else {
			return None;
		};
		let class_name = extends.to_identifier_name().unwrap().0.to_owned();
		Some(MxmlElement {
			namespace: self.use_namespace(&class_name),
			class_name,
			attributes,
			children: Vec::new(),
		})
	}

	/// checks for an `mx_internal::_document = this;` statement in the constructor
	fn is_valid_doc(&self) -> bool {
		let constructor = self.get_function(&self.class.name.0).unwrap();
		let body = constructor.common.body.as_ref().unwrap();
		let FunctionBody::Block(body) = body else {
			return false;
		};
		for directive in body.directives.iter() {
			let Directive::ExpressionStatement(expr) = directive.as_ref() else {
				continue;
			};
			let Expression::Assignment(expr) = expr.expression.as_ref() else {
				continue;
			};

			let mut correct_left = false;
			let mut correct_right = false;
			if let Expression::QualifiedIdentifier(identifier) = expr.left.as_ref() {
				if let Expression::QualifiedIdentifier(qualifier) = identifier.qualifier.as_ref().unwrap().as_ref() {
					if qualifier.to_identifier_name().unwrap().0 != "mx_internal" {
						continue;
					}
				};
				if let QualifiedIdentifierIdentifier::Id(id) = &identifier.id {
					if id.0 != "_document" {
						continue;
					}
				}
				correct_left = true;
			};
			if let Expression::ThisLiteral(_) = expr.right.as_ref() {
				correct_right = true;
			};

			if correct_left && correct_right {
				return true;
			}
		}
		false
	}

	/// generates and returns an mxml string
	fn generate_mxml(&self) -> Result<(), Box<dyn Error>> {
		let stdout = std::io::stdout().lock();
		let mut writer = EmitterConfig::new()
			.perform_indent(true)
			.create_writer(stdout);		

		let root = self.root.as_ref().unwrap();
		let events = root.get_parser_events(self)?;
		for event in events {
			writer.write(event)?;
		}
		Ok(())
	}
}

fn main() -> io::Result<()> {
	let entries = fs::read_dir("./files")?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;
	for path in entries {
		let file = fs::read_to_string(&path)
			.expect("error reading file");
		let parse_result = MxmlDoc::parse(&file);
		match parse_result {
			Some(document) => {
				let Some(_) = document.root else {
					continue;
				};
				let doc = document.generate_mxml();
				// write mxml
			},
			None => {
				// write original file
			}
		}
	}

	Ok(())
}
