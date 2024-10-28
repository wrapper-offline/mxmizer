use std::borrow::{Borrow, Cow};
use std::error::Error;
use std::str;
use xml::attribute::Attribute;
use xml::name::Name;
use xml::namespace::Namespace;
use xml::writer::{EmitterConfig, XmlEvent};

pub trait WriterEvents {
    fn get_writer_events(&self, doc:&MxmlDoc) -> Result<Vec<XmlEvent>, Box<dyn Error>>;
}
impl WriterEvents for AttributeChild {
    fn get_writer_events(&self, doc:&MxmlDoc) -> Result<Vec<XmlEvent>, Box<dyn Error>> {
		let mut events:Vec<XmlEvent> = Vec::new();

		let start = XmlEvent::StartElement {
			name: Name {
				local_name: &self.0,
				namespace: None,
				prefix: None,
			},
			attributes: Cow::from(Vec::new()),
			namespace: Cow::Owned(Namespace::empty()),
		};
		events.push(start.into());

		let mut child_events = self.1
			.get_writer_events(doc)
			.expect("Should have been able to get attribute child child events");
		events.append(&mut child_events);

		events.push(XmlEvent::end_element().into());
		Ok(events)
	}
}

#[derive(Debug, PartialEq)]
pub struct ElemAttribute(pub String, pub String);

#[derive(Debug, PartialEq)]
pub struct AttributeChild(pub String, pub MxmlElement);

pub struct MxmlDoc {
	pub root: MxmlElement,
	pub namespaces: Namespace,
	pub declarations: Vec<MxmlElement>,
}
impl MxmlDoc {
	/// generates and returns an mxml string
	pub fn generate_mxml(&self) -> Result<String, Box<dyn Error>> {
		let mut target: Vec<u8> = Vec::new();
		let mut writer = EmitterConfig::new()
			.perform_indent(true)
			.create_writer(&mut target);		

		let root = &self.root;

		let mut events:Vec<XmlEvent> = Vec::new();

		let start = XmlEvent::StartElement {
			name: Name {
				local_name: &self.root.class_name,
				namespace: None,
				prefix: Some(&self.root.namespace),
			},
			attributes: Cow::from(
				self.root.attributes.iter().map(|attr| Attribute {
					name: Name {
						local_name: attr.0.as_str(),
						namespace: None,
						prefix: None
					},
					value: attr.1.as_str()
				}).collect::<Vec<Attribute>>()
			),
			namespace: Cow::Owned(self.namespaces.to_owned()),
		};
		events.push(start.into());

		let dec_start = XmlEvent::StartElement {
			name: Name {
				local_name: "Declarations",
				namespace: None,
				prefix: Some("fx".into()),
			},
			attributes: Cow::from(Vec::new()),
			namespace: Cow::Owned(Namespace::empty()),
		};
		events.push(dec_start.into());
		for dec in &self.declarations {
			let mut child_events = dec
				.get_writer_events(self)
				.expect("Should have been able to get child events");
			events.append(&mut child_events);
		}
		events.push(XmlEvent::end_element().into());

		for child in &self.root.children {
			let mut child_events = child
				.get_writer_events(self)
				.expect("Should have been able to get child events");
			events.append(&mut child_events);
		}

		for attr_child in &self.root.attribute_children {
			let mut ac_events = attr_child
				.get_writer_events(self)
				.expect("Should have been able to get attribute child events");
			events.append(&mut ac_events);
		}

		events.push(XmlEvent::end_element().into());
		for event in events {
			writer.write(event)?;
		}


		let doc_str = match String::from_utf8(target) {
			Ok(v) => v,
			Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
		};
		Ok(doc_str)
	}
}

#[derive(Debug, PartialEq)]
pub struct MxmlElement {
	pub namespace: String,
	pub class_name: String,
	pub attributes: Vec<ElemAttribute>,
	pub attribute_children: Vec<AttributeChild>,
	pub children: Vec<MxmlElement>
}
impl MxmlElement {
	fn get_writer_events(&self, doc:&MxmlDoc) -> Result<Vec<XmlEvent>, Box<dyn Error>> {
		let mut events:Vec<XmlEvent> = Vec::new();

		let start = XmlEvent::StartElement {
			name: Name {
				local_name: &self.class_name,
				namespace: None,
				prefix: Some(&self.namespace),
			},
			attributes: Cow::from(
				self.attributes.iter().map(|attr| Attribute {
					name: Name {
						local_name: attr.0.as_str(),
						namespace: None,
						prefix: None
					},
					value: attr.1.as_str()
				}).collect::<Vec<Attribute>>()
			),
			namespace: Cow::Owned(
				if doc.root == *self {
					doc.namespaces.to_owned()
				} else {
					Namespace::empty()
				}
			),
		};
		events.push(start.into());

		for child in &self.children {
			let mut child_events = child
				.get_writer_events(doc)
				.expect("Should have been able to get child events");
			events.append(&mut child_events);
		}

		for attr_child in &self.attribute_children {
			let mut ac_events = attr_child
				.get_writer_events(doc)
				.expect("Should have been able to get attribute child events");
			events.append(&mut ac_events);
		}

		events.push(XmlEvent::end_element().into());
		Ok(events)
	}
}
