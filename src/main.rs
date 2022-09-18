#![feature(is_some_with)]
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
use std::sync::Mutex;

use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[macro_use]
extern crate lazy_static;

lazy_static! {
	static ref TYPE_REGEX: Regex = Regex::new(r#""type":"([A-Za-z]+)""#).unwrap();
	static ref STORAGE_MUT: Mutex<BTreeMap::<u64, RepoScore>> = {
		let mut m = BTreeMap::<u64, RepoScore>::new();
		Mutex::new(m)
	};
}

fn main() {
    let now = Instant::now();

    process(Path::new("D:/rust/workspace/bisne-test"), "2022-08-01", "gz")
        .unwrap_or_else(|x| println!("Error: {}", x));

    let elapsed = now.elapsed();
    println!("Elapsed: {:.2?}", elapsed);
	
	let storage = STORAGE_MUT.lock().unwrap();
    println!("count: {}", storage.len());
	let mut i = 0;
	for (id, repo) in storage.iter() {
		if (repo.prs_opened + repo.pushes + repo.commits) < 5 {
			continue;
		}
		println!("{}:\n\tprs: {}\n\tpushes: {}\n\tcommits: {}", repo.name, repo.prs_opened, repo.pushes, repo.commits);
		i += 1;
		if i >= 10 {
			break;
		}
	}
}


fn process(path: &Path, prefix: &str, extension: &str) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(&path)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|&e| e == OsStr::new(extension)))
    {
        if let Some(filename) = entry.file_name().to_str() {
            if filename.starts_with(prefix) {
                println!("Visiting file {}", filename);
                visit_file(File::open(entry.path())?);
            }
        }
    }

    return Ok(());
}

fn visit_file(file: File) {
	/*let gz = GzDecoder::new(BufReader::new(file));
    for line in BufReader::with_capacity(24 * 1024, gz).lines() {
		if let Ok(line) = line {
            parse_event(line.as_str());
        }
    }*/
	let mmap = unsafe { memmap2::Mmap::map(&file).expect("Error mapping file") };
	let gz = GzDecoder::new(&mmap[..]);
	let _found = thread_io::read::reader(256 * 1024, 8, gz, |reader| {
		let mut buf_reader = BufReader::with_capacity(256 * 1024, reader);
		let mut line = String::new();
		while buf_reader.read_line(&mut line)? > 0 {
            parse_event(line.as_str());
            line.clear();
        }
		Ok::<_, std::io::Error>(true)
	}).unwrap();
}

// repo score
struct RepoScore {
	id: u64,
	name: String,
	prs_opened: u64,
	pushes: u64,
	commits: u64,
}

fn new_repo_score(id: u64, name: &String) -> RepoScore {
	RepoScore {
		id: id,
		name: name.to_string(),
		prs_opened: 0,
		pushes: 0,
		commits: 0
	}
}

#[derive(Serialize, Deserialize)]
struct Event {
    r#type: String,
}

#[derive(Serialize, Deserialize)]
struct RepoInfo {
	id: u64,
	name: String
}

fn parse_event(event: &str) {
    //let event: Event = serde_json::from_str(event).unwrap();
    //println!("{}", event);
	//match mat.get(1).map_or("", |m| m.as_str()) {
	let mat = TYPE_REGEX.find(event).unwrap();
	match &event[mat.start()+8..mat.end()-1] {
		"PullRequestEvent" => {
			parse_pull_request_event(event);
		}
		"PushEvent" => {
			parse_push_event(event);
		}
		_ => {
			// untracked
		}
	}
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

fn parse_pull_request_event(event: &str) {
	let parsed_event: PullRequestEvent = serde_json::from_str(event).unwrap();
	match parsed_event.payload.action.as_str() {
		"opened" => {
			let mut storage = STORAGE_MUT.lock().unwrap();
			let repo = storage.entry(parsed_event.repo.id).or_insert(
				new_repo_score(parsed_event.repo.id, &parsed_event.repo.name)
			);
			repo.prs_opened += 1;
		}
		_ => {
			// untracked
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

fn parse_push_event(event: &str) {
	let parsed_event: PushEvent = serde_json::from_str(event).unwrap();
	if parsed_event.payload.distinct_size > 0 {
		let mut storage = STORAGE_MUT.lock().unwrap();
		let repo = storage.entry(parsed_event.repo.id).or_insert(
			new_repo_score(parsed_event.repo.id, &parsed_event.repo.name)
		);
		repo.pushes += 1;
		repo.commits += parsed_event.payload.distinct_size;
	}
}