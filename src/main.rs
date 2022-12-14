#![feature(is_some_with)]
#![feature(portable_simd)]
//use core::simd;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use regex::Regex;
use std::collections::BTreeMap;
use rayon::prelude::*;

#[global_allocator]
static ALLOC: rpmalloc::RpMalloc = rpmalloc::RpMalloc;

#[macro_use]
extern crate lazy_static;

lazy_static! {
	static ref TYPE_REGEX: Regex = Regex::new(r#""type":"([A-Za-z]+)""#).unwrap();
}


struct ParserInstance {
	regex: Regex,
	storage: Box<BTreeMap::<u64, RepoScore>>,
	path: String,
	date: u64
}

impl ParserInstance {
	fn new(path: &str, date: u64) -> ParserInstance {
		ParserInstance { regex: TYPE_REGEX.clone(), storage: Box::new(BTreeMap::<u64, RepoScore>::new()), path: String::from(path), date: date }
	}

	fn run(&mut self) -> Result<(), &'static str> {
		let p = Path::new(self.path.as_str());
		let f = File::open(p).unwrap();
		let gz = GzDecoder::new(BufReader::with_capacity(32*1024, f));
		let _found = thread_io::read::reader(256 * 1024, 8, gz, |reader| {
			let mut buf_reader = BufReader::with_capacity(256 * 1024, reader);
			let mut line = String::new();
			while buf_reader.read_line(&mut line)? > 0 {
				self.parse_event(line.as_mut_str());
				line.clear();
			}
			Ok::<_, std::io::Error>(true)
		}).unwrap();
		return Ok(());
	}

	fn parse_event(&mut self, event: &mut str) {
		let mat = self.regex.find(event);
		if let Some(mat) = mat {
			match &event[mat.start()+8..mat.end()-1] {
				"PullRequestEvent" => {
					self.parse_pull_request_event(event);
				}
				"PushEvent" => {
					self.parse_push_event(event);
				}
				_ => {
					// untracked
					// let parsed_event: Event = simd_json::from_str(event).unwrap();
				}
			}
		} else {
			// TODO: epic fail
		}
	}
}


fn main() {
    let now = Instant::now();

    process(Path::new("D:/rust/workspace/bisne-test/data"), "2022-08-01", "gz")
        .unwrap_or_else(|x| println!("Error: {}", x));

    let elapsed = now.elapsed();
    println!("Elapsed: {:.2?}", elapsed);
}


fn process(path: &Path, prefix: &str, extension: &str) -> Result<(), Box<dyn Error>> {
    fs::read_dir(&path)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|&e| e == OsStr::new(extension)))
		.par_bridge() // rayon moment
		.for_each(|entry| {
			if let Some(filename) = entry.file_name().to_str() {
				if filename.starts_with(prefix) {
					println!("Visiting file {}", filename);
					let mut parser = ParserInstance::new(entry.path().to_str().unwrap(), 0);
					parser.run().unwrap();
					let storage = parser.storage;
					println!("file {}: count: {}", filename, storage.len());
				}
			}
		});
    return Ok(());
}

// repo score
struct RepoScore {
	id: u64,
	name: String,
	prs_opened: u64,
	pushes: u64,
	commits: u64,
}

impl RepoScore {
	fn new(id: u64, name: &str) -> RepoScore {
		RepoScore {
			id: id,
			name: String::from(name),
			prs_opened: 0,
			pushes: 0,
			commits: 0
		}
	}

	fn fetch_or_new<'a>(storage: &'a mut BTreeMap::<u64, RepoScore>, id: u64, name: &str) -> &'a mut RepoScore {
		return storage.entry(id).or_insert(
			RepoScore::new(id, name)
		);
	}
}

// general


#[derive(Serialize, Deserialize)]
struct Event {
    r#type: String,
}


#[derive(Serialize, Deserialize)]
struct RepoInfo {
	id: u64,
	name: String
}

// PullRequestEvent
#[derive(Serialize, Deserialize)]
struct PullRequestEvent {
	repo: RepoInfo,
	payload: PullRequestEventPayload
}
#[derive(Serialize, Deserialize)]
struct PullRequestEventPayload {
	action: String
}

impl ParserInstance {
	fn parse_pull_request_event(&mut self, event: &mut str) {
		let parsed_event: PullRequestEvent = simd_json::from_str(event).unwrap();
		match parsed_event.payload.action.as_str() {
			"opened" => {
				let repo = RepoScore::fetch_or_new(
					self.storage.as_mut(), parsed_event.repo.id, parsed_event.repo.name.as_str()
				);
				repo.prs_opened += 1;
			}
			_ => {
				// untracked
			}
		}
	}
}

// PushEvent
#[derive(Serialize, Deserialize)]
struct PushEvent {
	repo: RepoInfo,
	payload: PushEventPayload
}
#[derive(Serialize, Deserialize)]
struct PushEventPayload {
	distinct_size: u64
}

impl ParserInstance {
	fn parse_push_event(&mut self, event: &mut str) {
		let parsed_event: PushEvent = simd_json::from_str(event).unwrap();
		if parsed_event.payload.distinct_size > 0 {
			let repo = RepoScore::fetch_or_new(
				self.storage.as_mut(), parsed_event.repo.id, parsed_event.repo.name.as_str()
			);
			repo.pushes += 1;
			repo.commits += parsed_event.payload.distinct_size;
		}
	}
}