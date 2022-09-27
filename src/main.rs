// to create a self-signed temporary cert for testing: `openssl req -x509 -newkey rsa:4096 -nodes -keyout key.pem -out cert.pem -days 365 -subj '/CN=localhost'`

use std::{io::BufReader, fs::File};
use rustls::{Certificate, PrivateKey, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys};

use actix_web::{get, web::{self}, App, HttpRequest, HttpServer, Responder,HttpResponse, http::header};
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use anyhow::{*, Result};
use clap::{Arg,  Command};
use {log::*, dotenv};
use mime;

const DEFAULT_IP : &str = "0.0.0.0";
const DEFAULT_PORT : u16 = 3000;
const DEFAULT_KEY_FILE : &str= "key.pem";
const DEFAULT_CERT_FILE : &str= "cert.pem";
const DEFAULT_CONNECTIONS : usize = 25*1024;

#[actix_web::main]
async fn main() -> Result<()> {
	env_logger::init();
	dotenv::dotenv().ok();

	let cmd = Command::new("bench_server")
						  .version("1.0")
						  .author("Xu Haojie <xuhaojie@hotmail.com>")
						  .about("A simple http(s) server for benchmark")
						  .arg(Arg::with_name("key")
						  	.short('k')
							.value_name("key")
							.takes_value(true)
						  	.help("Private key file, default key.pem, env key: KEY_FILE"))
						 .arg(Arg::with_name("cert")
						  	.short('c')
							.value_name("cert")
							.takes_value(true)
						  	.help("Certificate chain file, default cert.pem, env key: CERT_FILE"))
					
						  .arg(Arg::with_name("ip")
						  	.short('i')
							.value_name("ip")
							.takes_value(true)
						  	.help("Server bind ip, default 0.0.0.0, env key: SERVER_IP"))
						  .arg(Arg::with_name("port")
						  	.short('p')
							.value_name("http port")
							.takes_value(true)
							.default_missing_value("3080")
						  	.help("Http server port, default 3080, env key: HTTP_PORT"))
						  .arg(Arg::with_name("https")
						  	.short('s')
							.value_name("https port")
							.takes_value(true)
						  	.help("Enable and specify the https server port, env key: HTTPS_PORT"))
						  .arg(Arg::with_name("workers")
						  	.short('w')
							.value_name("workers")
							.takes_value(true)
						  	.help("Workers, default cpu core number, env key: WORKERS"))
						  .arg(Arg::with_name("max_connections")
						  	.short('m')
							.value_name("max_connections")
							.takes_value(true)
						  	.help("Max connections, default 25k, env key: CONNECTIONS"));

	let matches = cmd.get_matches();

	let key_file = match matches.value_of("key"){
		Some(file) => file.to_string(),
		_ => match dotenv::var("KEY_FILE") {
			dotenv::Result::Ok(file) => file,
			_ => DEFAULT_KEY_FILE.to_string(),
		}
	};

	let cert_file = match matches.value_of("cert"){
		Some(file) =>	file.to_string(),
		_ => match dotenv::var("CERT_FILE") {
			dotenv::Result::Ok(file) => file,
			_ => DEFAULT_CERT_FILE.to_string(),
		}
	};

	let server_ip = match matches.value_of("ip"){
		Some(ip) => ip.to_string(),
		_ => match dotenv::var("SERVER_IP") {
			dotenv::Result::Ok(ip) => ip,
			_ => DEFAULT_IP.to_string(),
		}
	};

	let http_port = match matches.value_of("port"){
		Some(port) => port.parse::<u16>()?,
		_ => match dotenv::var("HTTP_PORT") {
			dotenv::Result::Ok(port) => port.parse::<u16>()?,
			_ => DEFAULT_PORT,
		}
	};

	let https_port = match matches.value_of("https"){
		Some(port) => port.parse::<u16>()?,
		_ => match dotenv::var("HTTPS_PORT") {
			dotenv::Result::Ok(port) => port.parse::<u16>()?,
			_ => 0u16,
		}
	};

	let workers = match matches.value_of("workers"){
		Some(workers) => workers.parse::<usize>()?,
		_ => match dotenv::var("WORKERS") {
			dotenv::Result::Ok(workers) => workers.parse::<usize>()?,
			_ => 0,
		}
	};

	let connections = match matches.value_of("max_connections"){
		Some(connections) => connections.parse::<usize>()?,
		_ => match dotenv::var("CONNECTIONS") {
			dotenv::Result::Ok(connections) => connections.parse::<usize>()?,
			_ => DEFAULT_CONNECTIONS,
		}
	};

	let mut server = HttpServer::new(|| App::new()
	.configure(server_routes)
	.configure(benchmark_routes));
	
	if workers > 0 	{
		info!("set server workers to {}", workers);
		server = server.workers(workers);
	}

	if connections > 0 	{
		info!("set server max connections to {}", connections);
		server = server.max_connections(connections);
	}

	let http_address = format!("{}:{}", server_ip, http_port);

	info!("http server listen on {}", http_address);
	
	let server = server.bind(http_address)?;

	if https_port != 0 {				   
		let https_address = format!("{}:{}", server_ip, https_port);
		info!("https server listen on {}", https_address);
/*
		let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
		builder.set_private_key_file(key_file, SslFiletype::PEM)?;
	    builder.set_certificate_chain_file(cert_file)?;
		
		//server.bind_openssl(https_address, builder)?.run().await?;
*/

		let cert_file_ = &mut BufReader::new(File::open(cert_file)?);
		let key_file_ = &mut BufReader::new(File::open(key_file)?);
		
		let cert_chain = certs(cert_file_)?.into_iter().map(Certificate).collect();
		let mut keys: Vec<PrivateKey> = pkcs8_private_keys(key_file_)?.into_iter().map(PrivateKey).collect();
		
		if keys.is_empty() {
			return Err(anyhow!("Could not locate PKCS 8 private keys."));
		}
		
		let config = ServerConfig::builder().with_safe_defaults().with_no_client_auth();
		
		server.bind_rustls(https_address, config.with_single_cert(cert_chain, keys.remove(0))?)?.run().await?;
		//server.bind_rustls(https_address, builder)?.run().await?;
	} else {
		server.run().await?;
	}
	
	Ok(())

}


pub fn server_routes(cfg: &mut web::ServiceConfig) {
	cfg
	.route("/", web::get().to(index));
}

pub async fn index() -> HttpResponse  {
	HttpResponse::Ok()
	.insert_header(header::ContentType(mime::TEXT_HTML_UTF_8))
	.body(
r#"<!DOCTYPE html>
<html>
<head>
	<meta charset='utf-8'>
	<title>HTTP(S) Benchmark Server</title>
	<link rel='stylesheet' type='text/css' media='screen' href='main.css'>
	<script src='main.js'></script>
</head>
<body>
	
	<div style="text-align:center;">
		<H1>HTTP(S) Benchmark Server</H1>
	</div>
</body>
</html>"#
	)
}


pub fn benchmark_routes(cfg: &mut web::ServiceConfig) {
	cfg
	.route("/", web::get().to(index))
	.route("/get", web::get().to(bench_get))
	.route("/post", web::post().to(bench_post))
	.route("/put", web::put().to(bench_put))
	.route("/delete", web::delete().to(bench_delete));
}


pub async fn bench_get() -> HttpResponse  {
	HttpResponse::Ok()
	.insert_header(header::ContentType(mime::APPLICATION_JSON))
	.body(
r#"{
"args": {},
"headers": {
	"Accept": "application/json",
	"Accept-Encoding": "gzip, deflate",
	"Accept-Language": "en-US,en;q=0.5",
	"Host": "www.httpbin.org",
	"Referer": "http://www.httpbin.org/",
	"User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.14; rv:104.0) Gecko/20100101 Firefox/104.0",
	"X-Amzn-Trace-Id": "Root=1-632dce82-279a47540dd200b652a8cb02"
},
"origin": "113.200.214.222",
"url": "http://www.httpbin.org/get"
}"#)
}

pub async fn bench_post() -> HttpResponse  {
	HttpResponse::Ok()
	.insert_header(header::ContentType(mime::APPLICATION_JSON))
	.body(
r#"{
	"args": {},
	"data": "",
	"files": {},
	"form": {},
	"headers": {
		"Accept": "application/json",
		"Accept-Encoding": "gzip, deflate",
		"Accept-Language": "zh-cn",
		"Content-Length": "0",
		"Host": "httpbin.org",
		"Origin": "http://httpbin.org",
		"Referer": "http://httpbin.org/",
		"User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_14_6) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.1.2 Safari/605.1.15",
		"X-Amzn-Trace-Id": "Root=1-632dd138-2243642a76ba30163e857a96"
	},
	"json": null,
	"origin": "113.200.214.222",
	"url": "http://httpbin.org/put"
	}"#)
}

pub async fn bench_put() -> HttpResponse  {
	HttpResponse::Ok()
	.insert_header(header::ContentType(mime::APPLICATION_JSON))
	.body(
r#"{
	"args": {},
	"data": "",
	"files": {},
	"form": {},
	"headers": {
		"Accept": "application/json",
		"Accept-Encoding": "gzip, deflate",
		"Accept-Language": "zh-cn",
		"Content-Length": "0",
		"Host": "httpbin.org",
		"Origin": "http://httpbin.org",
		"Referer": "http://httpbin.org/",
		"User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_14_6) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.1.2 Safari/605.1.15",
		"X-Amzn-Trace-Id": "Root=1-632dd138-2243642a76ba30163e857a96"
	},
	"json": null,
	"origin": "113.200.214.222",
	"url": "http://httpbin.org/put"
	}"#)
}

pub async fn bench_delete() -> HttpResponse  {
	HttpResponse::Ok()
	.insert_header(header::ContentType(mime::APPLICATION_JSON))
	.body(
r#"{
	"args": {},
	"data": "",
	"files": {},
	"form": {},
	"headers": {
		"Accept": "application/json",
		"Accept-Encoding": "gzip, deflate",
		"Accept-Language": "zh-cn",
		"Content-Length": "0",
		"Host": "httpbin.org",
		"Origin": "http://httpbin.org",
		"Referer": "http://httpbin.org/",
		"User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_14_6) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.1.2 Safari/605.1.15",
		"X-Amzn-Trace-Id": "Root=1-632dd138-2243642a76ba30163e857a96"
	},
	"json": null,
	"origin": "113.200.214.222",
	"url": "http://httpbin.org/put"
	}"#)
}