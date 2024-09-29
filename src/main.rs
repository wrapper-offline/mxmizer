use std::fs;
use std::io;
use as3_parser::ns::*;

struct AsFile {
	// remove later
	#[allow(dead_code)]
	class: ClassDefinition
}

impl AsFile {
	fn parse(content:String) -> Result<AsFile, &'static str> {
		// parse file
		let compilation_unit = CompilationUnit::new(None, content.into());
		let parser_options = ParserOptions::default();
		let program = ParserFacade(&compilation_unit, parser_options).parse_program();

		// get class definition
		let package = program.packages.iter().next().unwrap();
		for directive in package.block.directives.iter() {
			match directive.as_ref() {
				Directive::ClassDefinition(defn) => {
					return Ok(AsFile {
						class: defn.to_owned()
					});
				},

				_ => {}
			}
		}

		Err("Could not find class definition")
	}

	/// checks for an `mx_internal::_document = this;` statement in the constructor
	fn is_mxml(&self) -> bool {
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
}

fn main() -> io::Result<()> {
	let entries = fs::read_dir("./files")?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;
	for path in entries {
		let file = fs::read_to_string(&path)?;
		let script = AsFile::parse(file)
			.expect("whoopsies");
		let is_mxml = script.is_mxml();
		println!("File {:?} is an MXML document: {}", &path, is_mxml);
	}

	Ok(())

	// let contents = fs::read_to_string(file_path)
	//	.expect("Should have been able to read the file");
}
