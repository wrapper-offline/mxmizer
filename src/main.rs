
mod mxml;
mod transpiler;

use crate::transpiler::MxmlTranspiler;
use std::env;
use std::fs;
use std::io;

fn main() -> Result<(), std::io::Error> {
	let args: Vec<String> = env::args().collect();

	for arg in args {
		println!("{}", arg);
	}

	let entries = fs::read_dir("./files")?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;
	for path in entries {
		let file = fs::read_to_string(&path)?;
		let parse_result = MxmlTranspiler::parse_doc(&file);
		match parse_result {
			Some(document) => {
				let doc = document.generate_mxml().unwrap();
				print!("{}", doc);
			},
			None => {
				// write original file
			}
		}
	}

	Ok(())
}
