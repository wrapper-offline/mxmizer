use std::borrow::Cow;
use std::fs;
use std::io;
use std::error::Error;
use as3_parser::ns::*;
use xml::name::Name;
use xml::namespace::Namespace;
use xml::writer::{EmitterConfig, XmlEvent};

const BLOCK_PROPS:[&str; 4] = ["_bindings", "_watchers", "_bindingsByDestination", "_bindingsBeginWithWord"];

#[derive(Debug)]
struct Attribute {
	name: String,
	value: String,
}
impl Attribute {
	fn parse(mut name:String, val_expr:&Expression) -> Option<Attribute> {
		if BLOCK_PROPS.contains(&name.as_str()) {
			return None;
		}
		let mut value = MxmlDocModel::expr_to_string(val_expr);
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
		Some(Attribute {
			name: name,
			value: value,
		})
	}
}

#[derive(Debug)]
struct MxmlElement {
	namespace: String,
	class_name: String,
	attributes: Vec<Attribute>,
	children: Vec<MxmlElement>
}
impl MxmlElement {
	fn get_parser_events(&self) -> Result<Vec<XmlEvent>, Box<dyn Error>> {
		let attributes = Vec::new();
		let mut namespace = Namespace::empty();
		namespace.put("a", "urn:some:document");
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

struct MxmlDocModel {
	class: ClassDefinition,
	root: Option<MxmlElement>,
}

impl MxmlDocModel {
	fn parse(content:&String) -> Option<MxmlDocModel> {
		// parse file
		let compilation_unit = CompilationUnit::new(None, content.into());
		let parser_options = ParserOptions::default();
		let program = ParserFacade(&compilation_unit, parser_options).parse_program();

		// get class definition
		let package = program.packages.iter().next().unwrap();
		for directive in package.block.directives.iter() {
			let Directive::ClassDefinition(defn) = directive.as_ref() else {
				continue;
			};
			let mut doc = MxmlDocModel {
				class: defn.to_owned(),
				root: None,
			};
			if !doc.is_valid_doc() {
				return None;
			}
			doc.root = doc.parse_constructor();
			return Some(doc);
		}

		None
	}

	fn expr_to_string(expr:&Expression) -> String {
		match expr {
			Expression::StringLiteral(lit) => return lit.value.to_owned(),
			Expression::NumericLiteral(lit) => return lit.value.to_owned(),
			_ => return "".into()
		}
	}

	fn parse_constructor(&mut self) -> Option<MxmlElement> {
		let constructor = self.get_function(&self.class.name.0).unwrap();
		let body = constructor.common.body.as_ref().unwrap();
		let FunctionBody::Block(body) = body else {
			return None;
		};
		let mut attributes:Vec<Attribute> = Vec::new();
		for expr in body.directives.iter() {
			let Directive::ExpressionStatement(expr) = expr.as_ref() else {
				continue;
			};
			match expr.expression.as_ref() {
				// possible: children, attributes, embeds,
				Expression::Assignment(expr) => {
					match expr.left.as_ref() {
						Expression::Member(left) => {
							// only accept properties of `this`
							let Expression::ThisLiteral(_) = left.base.as_ref() else {
								continue;
							};
							let QualifiedIdentifierIdentifier::Id(id) = &left.identifier.id else {
								continue;
							};

							if id.0 == "mxmlContent" {
								//TODO PARSE CHILDREn
								continue;
							}
							// attributes
							match Attribute::parse(id.0.clone(), expr.right.as_ref()) {
								Some(attr) => attributes.push(attr),
								None => continue,
							}
							continue;
						}
						// states
						Expression::QualifiedIdentifier(left) => {
							let QualifiedIdentifierIdentifier::Id(id) = &left.id else {
								continue;
							};
							if id.0 != "states" {
								continue;
							}
							// TODO PARE STAETS
						}
						_ => continue
					}
					// attributes

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
		Some(MxmlElement {
			namespace: "f*lla".into(),
			class_name: extends.to_identifier_name().unwrap().0.to_owned(),
			attributes: attributes,
			children: Vec::new()
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

	fn get_function(&self, name:&String) -> Option<&FunctionDefinition> {
		for directive in self.class.block.directives.iter() {
			match directive.as_ref() {
				Directive::FunctionDefinition(defn) => {
					if &defn.name_identifier().0 == name {
						return Some(defn);
					}
				}
				_ => {}
			}
		}
		None
	}

	/// generates and returns an mxml string
	fn generate_mxml(&self) -> Result<(), Box<dyn Error>> {
		let stdout = std::io::stdout().lock();
		let mut writer = EmitterConfig::new()
			.perform_indent(true)
			.create_writer(stdout);		

		let root = self.root.as_ref().unwrap();
		let events = root.get_parser_events()?;
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
		let parse_result = MxmlDocModel::parse(&file);
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
