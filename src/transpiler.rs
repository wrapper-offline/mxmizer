use as3_parser::ns::*;
use as3_parser::ns::Expression::{Assignment, QualifiedIdentifier, ThisLiteral};
use as3_parser::ns::QualifiedIdentifierIdentifier::Id;
use crate::mxml::{AttributeChild, ElemAttribute, MxmlDoc, MxmlElement};
use if_chain::if_chain;
use xml::namespace::Namespace;

const BLOCK_PROPS:[&str; 5] = ["_bindings", "_watchers", "_bindingsByDestination", "_bindingsBeginWithWord", "currentState"];
struct DefaultNamespace {
	base_package: &'static str,
	name: &'static str,
	value: &'static str,
}

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

struct Binding {
	dest_id: String,
	dest_attribute: String,
	source: String,
}

trait ChildDefinition {
    /// checks for an `mx_internal::_document = this;` statement
	/// in an element definition
	fn is_doc_elem(&self) -> bool;
}
impl ChildDefinition for FunctionDefinition {
    fn is_doc_elem(&self) -> bool {
		let body = self.common.body.as_ref().unwrap();
		let FunctionBody::Block(body) = body else {
			return false;
		};
		for directive in body.directives.iter() {
            let directive = directive.as_ref();
            let Directive::ExpressionStatement(expr) = directive else {continue};
            let Assignment(expr) = expr.expression.as_ref() else {continue};

            if let QualifiedIdentifier(left) = expr.left.as_ref() {
                // verify qualifier
                let Some(qualifier) = left.qualifier.as_ref() else {continue};
                let QualifiedIdentifier(qualifier) = qualifier.as_ref() else {continue};
                match qualifier.to_identifier_name() {
                    Some(qual_val) =>
                        if qual_val.0 != "mx_internal" { continue }
                    None => continue
                }
                // verify identifier
                let Id(id) = &left.id else {continue};
                if id.0 != "_document" {
                    continue;
                }
            } else {
                continue;
            }

            if let ThisLiteral(_) = expr.right.as_ref() {
                return true;
            }
        }
		false
	}
}

pub struct MxmlTranspiler {
	package:    Vec<String>,
    imports:    Vec<ImportDirective>,
    class:      ClassDefinition,
    namespaces: RefCell<Namespace>,
    bindings:   Vec<Binding>,
}
impl MxmlTranspiler {
    pub fn parse_doc(content:&String) -> Option<MxmlDoc> {
		// parse file
		let compilation_unit = CompilationUnit::new(None, content.to_owned());
		let parser_options = ParserOptions::default();
		let program = ParserFacade(&compilation_unit, parser_options).parse_program();

		let mut imports = Vec::new();
		let package = program.packages.iter().next()?;
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

                    let transpiler = MxmlTranspiler {
						package: MxmlTranspiler::package_name_to_vec(&package.name),
                        imports,
                        class: class.to_owned(),
                        namespaces: RefCell::new(namespace),
						bindings: Vec::new(),
                    };

					// verify that we're working with a valid mxml document
					let constructor = transpiler.get_function(&transpiler.class.name.0)?;
					if !constructor.is_doc_elem() {
						return None;
					}

					// TODO : store the bindings first so we can add them during element parsing
		
					let root = transpiler.parse_elem(constructor)?;

					// TODO : parse remaining functions so they can be added into a script tag

                    let doc = MxmlDoc {
						root,
						namespaces: transpiler.namespaces.into_inner(),
					};
					return Some(doc);
				},
				_ => continue,
			}
		}

		None
	}

	fn parse_elem(&self, elem_def:&FunctionDefinition) -> Option<MxmlElement> {
		let parse_as_root:bool = elem_def.name_identifier().0 == self.class.name.0;

		let body = elem_def.common.body.as_ref().unwrap();
		let FunctionBody::Block(body) = body else {return None};

		let extends_class:String;
		if parse_as_root {
			// get extended class name to use its package as a namespace
			let Expression::QualifiedIdentifier(extends) = self.class.extends_clause.as_ref().unwrap().as_ref() else {
				return None;
			};
			extends_class = extends.to_identifier_name().unwrap().0.to_owned();
		} else {
			// extract extended class name to use its package as a namespace
			if_chain! {
				if let Expression::QualifiedIdentifier(res_type) = elem_def
					.common.signature.result_type
					.as_ref()
					.expect("no return type specified")
					.as_ref();
				if let Some(res_type) = res_type.to_identifier_name();
				then {
					extends_class = res_type.0.to_owned();
				} else {
					// could not get element type
					return None;
				}
			}
		}
		let namespace = self.use_namespace(&extends_class);

		let mut attributes = Vec::new();
		let mut attribute_children = Vec::new();
		let mut children = Vec::new();

		let mut directives = body.directives.iter();
		// get the element initialization for additional verification
		// if we're not parsing the root
		let mut elem_var_name:Option<String> = None;
		if !parse_as_root {
			let elem_init = directives.next();
			let Some(elem_init) = elem_init else {return None};
			if_chain! {
				if let Directive::VariableDefinition(expr) = elem_init.as_ref();
				if let Expression::QualifiedIdentifier(id) = expr.bindings.first().unwrap().destructuring.destructuring.as_ref();
				then {
					elem_var_name = Some(id.to_identifier_name_or_asterisk().unwrap().0);
				}
			}
		}
		for expr in directives {
			let Directive::ExpressionStatement(expr) = expr.as_ref() else {continue};
			match expr.expression.as_ref() {
				Expression::Assignment(expr) => {
					if let Expression::Member(left) = expr.left.as_ref() {						
						if parse_as_root {
							// only accept properties of `this`
							let Expression::ThisLiteral(_) = left.base.as_ref() else {continue};
						} else {
							// only accept properties of the element variable
							let Expression::QualifiedIdentifier(qid) = left.base.as_ref() else {continue};
							if qid.to_identifier_name().unwrap().0 != elem_var_name.as_ref().unwrap().clone() {
								continue;
							}
						}
						let id:String = left.identifier.to_identifier_name().unwrap().0;

						// attribute children
						if let Expression::Call(ac_call) = expr.right.as_ref() {
							let Expression::Member(fn_name) = ac_call.base.as_ref() else {continue};
							let fn_name = fn_name.identifier.to_identifier_name();
							if let Some(fn_name) = fn_name {
								let func = self.get_function(&fn_name.0).unwrap();
								let attr_child = self.parse_attr_child(format!("{}:{}", namespace, id.clone()), func);
								if let Some(attr_child) = attr_child {
									attribute_children.push(attr_child);
								}
							}
							continue;
						}
						// children array
						if id == "mxmlContent" {
							//TODO PARSE CHILDREn
							let Expression::ArrayLiteral(right) = expr.right.as_ref() else {continue};
							let mut names = Vec::new();
							for child in &right.elements {
								if_chain! {
									if let Element::Expression(child) = child;
									if let Expression::Call(call) = child.as_ref();
									if let Expression::Member(member) = call.base.as_ref();
									if let QualifiedIdentifierIdentifier::Id(id) = &member.identifier.id;
									then {
										names.push(id.to_owned());
									}
								}
							}
							for func_name in names {
								let function = self
									.get_function(&func_name.0)
									.expect("child function is called in element definition but is not defined");
								let elem = self.parse_elem(function);
								if elem.is_some() {
									children.push(elem.unwrap());
								}
							}
							continue;
						}
						// embeds
						if parse_as_root && id.contains("_embed_mxml_") {
							// TOdo proper embed handler
							continue;
						}
						// attributes or properties
						if id == "id" {
							let Expression::StringLiteral(str) = &expr.right.as_ref() else {continue};
							// check if the id is actually set in the src
							// TODO: find a better way of checking this
							// match it to the method name - 2 characters?
							// TODO2: see if anything goes against this
							if elem_def.name_identifier().0.starts_with(&str.value) {
								continue;
							}
						}
						match MxmlTranspiler::parse_attr(id.clone(), expr.right.as_ref()) {
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
		
		Some(MxmlElement {
			namespace,
			class_name: extends_class,
			attributes,
			attribute_children,
			children,
		})
	}

	fn parse_attr(mut name:String, val_expr:&Expression) -> Option<ElemAttribute> {
		if BLOCK_PROPS.contains(&name.as_str()) {
			return None;
		}
		let mut value = MxmlTranspiler::expr_to_string(val_expr);
		match name.as_str() {
			"percentWidth" => {
				name = "width".into();
				value += "%";
			}
			"percentHeight" => {
				name = "height".into();
				value += "%";
			}
			"color" => {
				value = format!("0x{:X}", value.parse::<i32>().unwrap());
			}
			val => name = String::from(val.to_owned())
		}
		Some(ElemAttribute(name, value))
	}

	fn parse_attr_child(&self, name:String, val:&FunctionDefinition) -> Option<AttributeChild> {
		let elem = self.parse_elem(val)?;
		Some(AttributeChild(name, elem))
	}

	fn use_namespace(&self, specifier:&String) -> String {
		let full_name = self
			.get_class_package(specifier)
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

	fn get_class_package(&self, class_name:&String) -> Option<Vec<String>> {
		for import in self.imports.iter() {
			if_chain! {
				if let ImportSpecifier::Identifier(id) = &import.import_specifier;
				if &id.0 == class_name;
				then {
					let name = MxmlTranspiler::package_name_to_vec(&import.package_name);
					return Some(name);
				}
			}
		}
		// no import statement found, default to component package
		Some(self.package.clone())
	}

    fn package_name_to_vec(name: &Vec<(String, Location)>) -> Vec<String> {
		name.to_owned()
			.iter_mut()
			.map(|v| return v.0.to_owned())
			.collect::<Vec<String>>()
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
			Expression::Unary(operation) => {
				let operator:&str;
				match operation.operator {
					Operator::Negative => {
						operator = &"-";
					}
					_ => panic!("Unknown unary operator!")
				}
				return format!("{}{}", operator, MxmlTranspiler::expr_to_string(operation.expression.as_ref()).as_str());
			},
			_ =>return "".into()
		}
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

    
}
