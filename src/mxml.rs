use std::borrow::Cow;
use std::error::Error;
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
}
impl MxmlDoc {
	/// generates and returns an mxml string
	pub fn generate_mxml(&self) -> Result<(), Box<dyn Error>> {
		let stdout = std::io::stdout().lock();
		let mut writer = EmitterConfig::new()
			.perform_indent(true)
			.create_writer(stdout);		

		let root = &self.root;
		let events = root.get_writer_events(self)?;
		for event in events {
			writer.write(event)?;
		}
		Ok(())
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
	/// returns a vector of `XmlEvent`s to be used by xml-rs
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
