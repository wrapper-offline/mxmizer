use std::fs;
use std::io;
use as3_parser::ns::*;

struct Attribute {
	name: String,
	value: String,
}

struct MxmlElement {
	namespace: String,
	class: String,
	attributes: Vec<Attribute>,
	children: Vec<MxmlElement>
}

const BLOCK_PROPS:[&str; 4] = ["_bindings", "_watchers", "_bindingsByDestination", "_bindingsBeginWithWord"];

struct MxmlDoc {
	class: ClassDefinition,
	root: Option<MxmlElement>,
	#[allow(unused)]
	children: Option<Vec<MxmlElement>>,
}

impl MxmlDoc {
	fn parse(content:&String) -> Result<MxmlDoc, &'static str> {
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
			let mut doc = MxmlDoc {
				class: defn.to_owned(),
				root: None,
				children: None,
			};
			if !doc.is_valid_doc() {
				return Err("heh");
			}
			doc.parse_constructor();
			return Ok(doc);
		}

		Err("NO_DEF")
	}

	fn expr_to_string(expr:&Expression) -> String {
		match expr {
			Expression::StringLiteral(lit) => return lit.value.to_owned(),
			Expression::NumericLiteral(lit) => return lit.value.to_owned(),
			_ => return "".into()
		}
	}

	fn parse_constructor(&mut self) {
		let constructor = self.get_function(&self.class.name.0).unwrap();
		let body = constructor.common.body.as_ref().unwrap();
		let FunctionBody::Block(body) = body else {
			return;
		};
		let mut attributes:Vec<Attribute> = Vec::new();
		for directive in body.directives.iter() {
			let Directive::ExpressionStatement(expr) = directive.as_ref() else {
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
							if BLOCK_PROPS.contains(&id.0.as_str()) {
								continue;
							}
							let name;
							match id.0.to_owned().as_str() {
								"percentWidth" => name = "width".into(),
								"percentHeight" => name = "height".into(),
								val => name = String::from(val.to_owned())
							}
							let value = MxmlDoc::expr_to_string(expr.right.as_ref());
							attributes.push(Attribute {
								name: name,
								value: value,
							});
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
		self.root = Some(MxmlElement {
			namespace: "hi".into(),
			class: "yo".into(),
			attributes: attributes,
			children: Vec::new()
		});
	}

	fn parse_child(&self, name:String) {

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

	fn generate_mxml() {

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
			Ok(document) => {

			},
			Err(err) => {
				if err == "INVALID" {
					println!("{:?} - Skipped", path);
					continue;
				}
				
			}
		}
	}

	Ok(())
}
