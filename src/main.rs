extern crate serde_json;
extern crate regex;
extern crate native_tls;

use serde_json::Value;
use regex::Regex;
use native_tls::TlsConnector;

use std::io::prelude::*;
use std::net::TcpStream;
use std::thread;
use std::sync::{Mutex, Arc};
use std::fs::File;
use std::env;

static DEBUG: bool = false;
static REGIONS: [&'static str; 2] = ["eune", "euw"];


fn download_page(region: &str, request: String) -> Vec<String> {
	let base_url = format!("boards.{}.leagueoflegends.com", region);
	let mut conn = TcpStream::connect(format!("{}:80", base_url)).unwrap();

	let req = format!(
		"GET {} HTTP/1.0\r\nHost: {}\r\nContent-Type: text/html; charset=utf-8\r\nUser-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:60.0) Gecko/20100101 nemin/1.0\r\n\r\n", 
		request, 
		base_url
	);

	conn.write(req.as_bytes()).unwrap();
	
	let mut resp: Vec<u8> = Vec::new();
	conn.read_to_end(&mut resp).unwrap();
	
	let resp_full = String::from_utf8(resp).unwrap();
	let split = resp_full.split("\r\n\r\n").map(|val| val.to_string()).collect::<Vec<String>>();

	split
}

fn find_names(namelist: &serde_json::Value, list: &mut Vec<String>, depth: usize) {
	for val in namelist["comments"].as_array().unwrap() {
		let name = (val["user"]["name"]).clone().to_string();
		
		if !list.contains(&name) {
			if DEBUG {println!("Name added - {}", &name);}
			list.push(name);
		}
		
		find_names(&val["replies"], list, depth+1);
	}
}

fn make_api_request(region: &str, location: &str) -> serde_json::Value {
	let data = download_page(region, location.to_string());
	
	if DEBUG {println!("API Request!");}
	
	serde_json::from_str(&data[1]).unwrap()
}

fn collect_posts(region: &'static str, url: &str, count: usize) -> Vec<(usize, String)> {
	//data-application-id=\"YzZuykbk\" data-discussion-id=\"e0ekEb3Z\"
	let app_id = Regex::new(r#"data-application-id=\\"(.*?)\\""#).unwrap();
	let disc_id = Regex::new(r#"data-discussion-id=\\"(.*?)\\""#).unwrap();
	
	let mut a_ids = Vec::new();
	let mut d_ids = Vec::new();

	
	for page in 0..count {
		let request = format!("{}?num_loaded={}&sort_type=recent", url, page*50);
		let data = download_page(region, request);

		for cap in app_id.captures_iter(&data[1]) {
			a_ids.push(cap[1].to_string().clone());
		}
		
		for cap in disc_id.captures_iter(&data[1]) {
			d_ids.push(cap[1].to_string().clone());
		}
	}
	
	assert_eq!(a_ids.len(), d_ids.len());

	let mut thread_vec = Vec::new();
	
	let length = a_ids.len();
	
	let a_thread = Arc::new(Mutex::new(a_ids));
	let d_thread = Arc::new(Mutex::new(d_ids));
	
	let names: Vec<String> = Vec::new();
	let names_thread = Arc::new(Mutex::new(names));

	print!("{}[33;46m", 27 as char);
	println!("\n\n ---- THREADS ----\n");
	print!("{}[39;49m", 27 as char);

	print!("{}[33;40m", 27 as char);
	for i in 0..length {
		let a_thread = Arc::clone(&a_thread);
		let d_thread = Arc::clone(&d_thread);
		let names_thread = Arc::clone(&names_thread);
	
		thread_vec.push(thread::spawn(move || {
			let a = a_thread.lock().unwrap();
			let d = d_thread.lock().unwrap();
			let mut names = names_thread.lock().unwrap();
		
			println!("Starting thread: [{}/{}]", a[i], d[i]);

			let req: Value = make_api_request(&region, &format!("/api/{}/discussions/{}", a[i], d[i]));	
			find_names(&req["discussion"]["comments"], &mut names, 0);
		}));
	}

	for th in thread_vec {
		th.join().unwrap();
	}
	print!("{}[39;49m", 27 as char);

	print!("{}[33;46m", 27 as char);
	println!("\n\n---- REQUESTS ----\n");
	print!("{}[39;49m", 27 as char);

	let mut retval = Vec::new();

	let name_clone = Arc::clone(&names_thread);
	let names2 = name_clone.lock().unwrap();
	for name in &*names2 {
		let points = get_points(region, &str::replace(&name.to_string(), "\"", ""), false);

		if points != 0 {
			let result: (usize, String) = (points, str::replace(&name.to_string(), "\"", ""));
			if DEBUG {println!("{} - {}", result.0, result.1);}

			retval.push(result);
		}
	}

	retval
}

fn get_points(region: &str, name: &str, retry: bool) -> usize {
	let base_url = format!("boards.{}.leagueoflegends.com", region);

    let connector = TlsConnector::builder().unwrap().build().unwrap();
	let conn = TcpStream::connect(format!("{}:443", base_url)).unwrap();
	let mut conn = connector.connect(&base_url, conn).unwrap();
		
	let request = format!("GET /en/player/{}/{} HTTP/1.0\r\nHost: {}\r\nUser-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:60.0) Gecko/20100101 nemin/1.0\r\n\r\n", 
		region,
		name,
		base_url
	);

	conn.write_all(request.as_bytes()).unwrap();
	
	if DEBUG {println!("Request sent!");}
	
	let mut resp: Vec<u8> = Vec::new();
	conn.read_to_end(&mut resp).unwrap();
	
	let resp_full = String::from_utf8(resp).unwrap();
	let points = Regex::new(r#"lifetime-upvotes">\n\t\t<span class="number opaque" data-short-number="(.+)">"#).unwrap();
	
	if DEBUG {println!("Get points!");}
	
	return match points.captures(&resp_full) {
		Some(pts) => {
			print!("{}[32;40m", 27 as char);
			println!("\"{}\" found on {}. Upvotes: {}", name, &region.to_ascii_uppercase(), &pts[1]);
			print!("{}[39;49m", 27 as char);
			pts[1].parse::<usize>().unwrap_or(0)
		},
		None => {
			if retry == false {
				print!("{}[33;40m", 27 as char);
				if region == "eune" {
					println!("\"{}\" was not found on EUNE. Retrying on EUW.", name); 
					print!("{}[39;49m", 27 as char);
					get_points("euw", name, true)
				} else if region == "euw" {
					println!("\"{}\" was not found on EUW. Retrying on EUNE.", name);
					print!("{}[39;49m", 27 as char);
					get_points("eune", name, true) 
				} else {unimplemented!()}
			} else {
				print!("{}[31;40m", 27 as char);
				println!("\"{}\" can not be found. Skipping.", name);
				print!("{}[39;49m", 27 as char);
				0
			}
		}
	}
}

fn main() {	
	//let decoded: Value = make_api_request("/api/YzZuykbk/discussions/G7v9hVxQ");
	//let decoded2: Value = make_api_request("/api/YzZuykbk/discussions/kfyaQhN4");
	
	//find_names(&decoded["discussion"]["comments"], 0);
	
	//println!("\n\n");
	
	//find_names(&decoded2["discussion"]["comments"], 0);
	
	//println!("{}", get_points("eune", "Gamma Ray"));

	//EUW/EUNE - en: "/api/0oazE84H/discussions"
	//EUNE - hu: "/api/q98U6Ykw/discussions"
	let arg: Vec<_> = env::args().collect();

	if arg.len() != 4 {
		println!("BoardsCount - A small tool to print out Boards users' scores in order.\nUsage: {} [region (eune/euw)] [API endpoint (get it from 'box-show-more')] [Number of pages to process (I recommend max. 20, it's gonna be real slow)]",
			&arg[0]
		);
	} else {
		let region = if &arg[1] == "eune" {0} else {1};
		let api_end = &arg[2];
		let pages = arg[3].parse::<usize>().unwrap();

		let mut vals: Vec<(usize, String)> = collect_posts(REGIONS[region], &api_end, pages);
		vals.sort_unstable_by_key(|v| v.0);
		vals.reverse();

		let mut outp = File::create("ranking.txt").unwrap();

		print!("{}[33;46m", 27 as char);
		println!("\n\n---- FINAL ----\n");
		print!("{}[39;49m", 27 as char);

		let mut counter = 1;
		for val in vals {
			let formatted = format!("{}. {} - {}\r\n", counter, val.1, val.0);

			print!("{}", formatted);
			outp.write_all(formatted.as_bytes()).unwrap();
			counter += 1;
		}

		outp.flush().unwrap();
	}
	//println!("{}", get_points("Nemin"));
}
