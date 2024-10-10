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

const BLOCK_PROPS:[&str; 5] = ["_bindings", "_watchers", "_bindingsByDestination", "_bindingsBeginWithWord", "currentState"];
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
	/// parses a child element definition
	fn parse(doc:&MxmlDoc, elem_def:&FunctionDefinition) -> Option<MxmlElement> {
		let parse_as_root:bool = elem_def.name_identifier().0 == doc.class.name.0;

		let body = elem_def.common.body.as_ref().unwrap();
		let FunctionBody::Block(body) = body else {
			return None;
		};

		let mut attributes = Vec::new();
		let mut children = Vec::new();

		let mut directives = body.directives.iter();
		let mut elem_var_name:Option<String> = None;
		if !parse_as_root {
			let elem_init = directives.next();
			match elem_init {
				Some(elem_init) => {
					if_chain! {
						if let Directive::VariableDefinition(expr) = elem_init.as_ref();
						if let Expression::QualifiedIdentifier(id) = expr.bindings.first().unwrap().destructuring.destructuring.as_ref();
						then {
							elem_var_name = Some(id.to_identifier_name_or_asterisk().unwrap().0);
						}
					}
				}
				None => return None
			}
		}
		for expr in directives {
			let Directive::ExpressionStatement(expr) = expr.as_ref() else {
				continue;
			};
			match expr.expression.as_ref() {
				Expression::Assignment(expr) => {
					if let Expression::Member(left) = expr.left.as_ref() {
						let id:(String, Location);
						// only accept properties of `this`
						if parse_as_root {
							let Expression::ThisLiteral(_) = left.base.as_ref() else {continue};
							id = left.identifier.to_identifier_name().unwrap();
						} else {
							let Expression::QualifiedIdentifier(qid) = left.base.as_ref() else {continue};
							if qid.to_identifier_name().unwrap().0 != elem_var_name.as_ref().unwrap().clone() {continue}
							id = left.identifier.to_identifier_name().unwrap();
						}
						
						// children array
						if id.0 == "mxmlContent" {
							//TODO PARSE CHILDREn
							let Expression::ArrayLiteral(right) = expr.right.as_ref() else {
								continue;
							};
							let names = MxmlElement::child_func_names(right);
							for func_name in names {
								let function = doc
									.get_function(&func_name)
									.expect("child function is called in element definition but is not defined");
								let elem = MxmlElement::parse(doc, function);
								if elem.is_some() {
									children.push(elem.unwrap());
								}
							}
							continue;
						}
						// embeds
						if id.0.contains("_embed_mxml_") {
							if !parse_as_root {
								continue;
							}
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
					// states
					if_chain! {
						if let Expression::QualifiedIdentifier(left) = expr.left.as_ref();
						if let QualifiedIdentifierIdentifier::Id(id) = &left.id;
						if id.0 != "states";
						if parse_as_root;
						then {
							// TODO PARE STAETS
						} else {
							continue;
						}
					}
				}
				// component declarations
				Expression::Call(expr) => {
					if !parse_as_root {
						continue;
					}
				}
				_ => {}
			}
		}
		let class_name:String;
		if parse_as_root {
			let Expression::QualifiedIdentifier(extends) = doc.class.extends_clause.as_ref().unwrap().as_ref() else {
				return None;
			};
			class_name = extends.to_identifier_name().unwrap().0.to_owned();
		} else {
			if_chain! {
				if let Expression::QualifiedIdentifier(res_type) = elem_def
					.common.signature.result_type
					.as_ref()
					.expect("no return type specified")
					.as_ref();
				if let Some(res_type) = res_type.to_identifier_name();
				then {
					class_name = res_type.0.to_owned();
				} else {
					// could not get element type
					return None;
				}
			}
		}
		
		Some(MxmlElement {
			namespace: doc.use_namespace(&class_name),
			class_name,
			attributes,
			children,
		})
	}

	/// parses an array of this method calls and returns a vector of
	/// the function names
	fn child_func_names(array:&ArrayLiteral) -> Vec<String> {
		let mut names = Vec::new();
		for child in &array.elements {
			if_chain! {
				if let Element::Expression(child) = child;
				if let Expression::Call(call) = child.as_ref();
				if let Expression::Member(member) = call.base.as_ref();
				if let QualifiedIdentifierIdentifier::Id(id) = &member.identifier.id;
				then {
					names.push(id.0.to_owned());
				}
			}
		}
		names
	}

	fn get_writer_events(&self, doc:&MxmlDoc) -> Result<Vec<XmlEvent>, Box<dyn Error>> {
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
			namespace = doc.namespaces.borrow().to_owned();
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

		for child in &self.children {
			let mut child_events = child
				.get_writer_events(doc)
				.expect("Should have been able to get child events");
			events.append(&mut child_events);
		}

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
	name: Vec<String>,
	imports: Vec<ImportDirective>,
	class: ClassDefinition,
	root: Option<MxmlElement>,
	namespaces: RefCell<Namespace>,
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
						name: MxmlDoc::package_name_to_vec(&package.name),
						imports,
						class: class.to_owned(),
						root: None,
						namespaces: RefCell::new(namespace),
						bindings: Vec::new(),
					};
					let constructor = doc.get_function(&doc.class.name.0).unwrap();
					if !MxmlDoc::is_doc_elem(constructor) {
						return None;
					}
					// store the bindings first so we can use them during element parsing
					// doc.bindings
		
					doc.root = MxmlElement::parse(&doc, constructor);
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

	fn package_name_to_vec(name: &Vec<(String, Location)>) -> Vec<String> {
		name.to_owned()
			.iter_mut()
			.map(|v| return v.0.to_owned())
			.collect::<Vec<String>>()
	}

	/// returns the name of an imported package as a vector of strings
	fn get_package_name(&self, specifier:&String) -> Option<Vec<String>> {
		for import in self.imports.iter() {
			if_chain! {
				if let ImportSpecifier::Identifier(id) = &import.import_specifier;
				if &id.0 == specifier;
				then {
					let name = MxmlDoc::package_name_to_vec(&import.package_name);
					return Some(name);
				}
			}
		}
		// no import statement found, default to component directory
		Some(self.name.clone())
	}

	fn use_namespace(&self, specifier:&String) -> String {
		let full_name = self
			.get_package_name(specifier)
			.expect(("Import for ".to_owned() + specifier + " should have existed").as_str());

		let first_val = full_name
			.first()
			.expect("Package name should have had at least one element");
		let def_ns = DEFAULT_NAMESPACES.into_iter().find(|ns| ns.base_package == first_val);
		if let Some(def_ns) = def_ns {
			return def_ns.name.into();
		}

		let name = full_name.last().unwrap().to_owned();
		let value = full_name.join(".") + ".*";
		self.namespaces.borrow_mut().put(name.clone(), value);
		name
	}

	/// checks for an `mx_internal::_document = this;` statement
	/// in an element definition
	fn is_doc_elem(elem_def:&FunctionDefinition) -> bool {
		let body = elem_def.common.body.as_ref().unwrap();
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
		let events = root.get_writer_events(self)?;
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
