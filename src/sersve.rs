#![feature(plugin, path_ext, path_relative_from)]
#![plugin(serde_macros, docopt_macros)]

extern crate iron;
extern crate regex;
extern crate conduit_mime_types;
extern crate mustache;
extern crate libc;
extern crate num_cpus;
extern crate serde;
#[macro_use]
extern crate lazy_static;
extern crate docopt;
extern crate rustc_serialize;
extern crate env_logger;
extern crate serde_json;

use std::{ env, fs, process };
use std::path::{ Path, PathBuf };
use std::io::{ Read, Write };
use std::error::Error;
use std::fs::{ File, PathExt };
use std::sync::Arc;
use std::borrow::Borrow;

use regex::Regex;

use conduit_mime_types::Types;

use serde_json::Value;

use iron::prelude::*;
use iron::status;
use iron::mime::{ Mime, TopLevel, SubLevel };
use iron::headers::ContentType;
use iron::modifiers::Header;

use mustache::{ Template, VecBuilder, MapBuilder };

use constants::*;

pub mod constants;

docopt!(Args derive Debug, "
A minimal static file server, written in Rust with Iron.
Usage: sersve [options]

Options:
    -h, --help                  Show this message.
    -v, --version               Show the version of sersve (duh).
    -c, --config FILE           Provide a configuration file (JSON).
    -a, --address HOST          The address to bind to.
    -p, --port PORT             The port to serve.
    -r, --root ROOT             The uppermost directory to serve.
    -f, --filter REGEX          A regular expression to filter the filenames.
    -s, --size BYTES            The maximum size of a file that will be served.
    -t, --template TEMPLATE     A Mustache template to use for rendering.
    --threads THREADS           Amount of threads to use for serving.
    --fork                      Fork sersve into a background process.",
    flag_help: bool,
    flag_version: bool,
    flag_config: Option<String>,
    flag_address: Option<String>,
    flag_port: Option<u16>,
    flag_root: Option<String>,
    flag_filter: Option<String>,
    flag_size: Option<u64>,
    flag_template: Option<String>,
    flag_threads: Option<usize>,
    flag_fork: bool
);

#[derive(Clone)]
struct State {
    template: Template,
    root: Option<PathBuf>,
    mime_types: Arc<Types>
}

const HOST: &'static str = "0.0.0.0";
const PORT: u16 = 8080;

static UNITS: &'static [&'static str] = &["B", "kB", "MB", "GB", "TB"];

const KEY_TITLE: &'static str = "title";
const KEY_CONTENT: &'static str = "content";
const KEY_URL: &'static str = "url";
const KEY_SIZE: &'static str = "size";
const KEY_NAME: &'static str = "name";

const DEF_LEN: usize = 10000;

lazy_static! {
    static ref ARGS: Args = {
        let mut args: Args = Args::docopt().decode().unwrap_or_else(|e| e.exit());

        if let Some(ref flag_config) = args.flag_config {
            let conf = File::open(&flag_config)
                            .and_then(|mut f| {
                                let mut out = String::new();
                                f.read_to_string(&mut out).map(|_| out)
                            }).map_err(|e| error(e.description()))
                            .unwrap();

            // cannot if-let, because typesafe errors are helpful
            let json = match serde_json::from_str(&conf) {
                Ok(Value::Object(o)) => o,
                _ => panic!("Invalid configuration file. Doesn't contain valid top-level object.")
            };
            args.flag_address = args.flag_address.or(match json.get("address") {
                Some(&Value::String(ref s)) => Some((*s).clone()),
                None => None,
                _ => panic!("Invalid configuration file. `address` field must be a string.")
            });
            args.flag_port = args.flag_port.or(match json.get("port") {
                Some(&Value::U64(u)) => Some(u as u16),
                None => None,
                _ => panic!("Invalid configuration file. `port` field must be an unsigned integer.")
            });
            args.flag_root = args.flag_root.or(match json.get("root") {
                Some(&Value::String(ref s)) => Some(s.clone()),
                None => None,
                _ => panic!("Invalid configuration file. `root` field must be a string.")
            });
            args.flag_filter = args.flag_filter.or(match json.get("filter") {
                Some(&Value::String(ref s)) => Some((*s).clone()),
                None => None,
                _ => panic!("Invalid configuration file. `filter` field must be a string.")
            });
            args.flag_size = args.flag_size.or(match json.get("size") {
                Some(&Value::U64(u)) => Some(u),
                None => None,
                _ => panic!("Invalid configuration file. `size` field must be an unsigned integer.")
            });
            args.flag_template = args.flag_template.or(match json.get("template") {
                Some(&Value::String(ref s)) => Some((*s).clone()),
                None => None,
                _ => panic!("Invalid configuration file. `template` field must be a string.")
            });
            args.flag_fork = args.flag_fork || match json.get("fork") {
                Some(&Value::Bool(b)) => b,
                None => false,
                _ => panic!("Invalid configuration file. `fork` field must be a boolean")
            };
            args.flag_threads = args.flag_threads.or(match json.get("threads") {
                Some(&Value::U64(u)) => Some(u as usize),
                None => None,
                _ => panic!("Invalid configuration file. `threads` field must be a string.")
            })
        };

        args
    };

    static ref STATE: State = State {
        template: mustache::compile_str(&ARGS.flag_template.as_ref().unwrap_or(&OPT_TEMPLATE.to_owned())),
        root: ARGS.flag_root.clone().map(|p| Path::new(&p).to_path_buf()),
        mime_types: Arc::new(Types::new().ok().unwrap())
    };
}

fn error(e: &str) -> ! {
    println!("Error: {}", e);
    process::exit(-1);
}

fn fork() {
    unsafe {
        let pid = libc::funcs::posix88::unistd::fork();
        if pid == 0 {
            // we are child, now get to work
            return;
        } else if pid > 0 {
            // fork succeeded, die
            process::exit(0);
        } else if pid < 0 {
            // unsuccessful, don't die
            return;
        }
    }
}

fn size_with_unit(mut size: u64) -> String {
    let mut frac = 0;
    let mut index = 0;

    while size > 1000 && index + 1 < UNITS.len() {
        frac = size % 1000;
        size /= 1000;
        index += 1;
    }

    format!("{}.{} {}", size, frac, UNITS[index])
}

fn render<'a, W: Write>(mut out: W, template: Template, root: PathBuf, dir: PathBuf, files: Vec<PathBuf>, filter: Option<Regex>) {
    let data = MapBuilder::new()
        .insert_str(KEY_TITLE, format!("{}", dir.display()))
        .insert_vec(KEY_CONTENT, |mut vec: VecBuilder| {
            let item = |map: MapBuilder, url: &Path, size: u64, name: String| {
                map.insert(KEY_URL, &format!("{}", url.display())).unwrap()
                   .insert(KEY_SIZE, &size_with_unit(size)).unwrap()
                   .insert_str(KEY_NAME, name)
            };

            // add `..` entry if necessary
            let mut up = dir.to_path_buf();
            up.pop();
            if up.starts_with(&root) {
                vec = vec.push_map(|map: MapBuilder| item(map, &up.relative_from(&root).unwrap(), 0, "..".to_owned()));
            }

            for file in files.iter() {
                let relative = file.relative_from(&root).unwrap();
                let stat = file.metadata().unwrap();
                let filename = file.file_name()
                    .expect("Cannot get filename").to_string_lossy().into_owned();
                if filter.as_ref().map_or(true, |f| f.is_match(&filename)) {
                    vec = vec.push_map(|map| item(map, &relative, stat.len(), filename.clone()));
                }
            }
            vec
        }).build();

    template.render_data(&mut out, &data);
}

fn plain(content: &[u8]) -> IronResult<Response> {
    Ok(Response::with((status::Ok, content)))
}

fn html(content: &[u8]) -> IronResult<Response> {
    plain(content).map(|r| r.set(Header(ContentType(Mime(TopLevel::Text, SubLevel::Html, vec![])))))
}

fn from_path(path: &Path) -> IronResult<Response> {
    Ok(Response::with((status::Ok, path)))
}

fn serve(req: &mut Request) -> IronResult<Response> {
    req.headers.set_raw("Connection", vec![b"close".to_vec()]);

    let (filter_str, max_size) = (
        ARGS.flag_filter.clone(),
        ARGS.flag_size
    );

    let (template, root, mime_types) = (
        STATE.template.clone(),
        STATE.root.clone().unwrap_or_else(|| env::current_dir().ok().unwrap()),
        STATE.mime_types.clone()
    );

    let mut path = root.clone();
    for part in req.url.path.iter() { path.push(part) }
    if !path.exists() { return html(format!("Error, `{}` does not exist.", path.display()).as_bytes()); }

    let filter = filter_str.and_then(|s| Regex::new(&s).ok());

    if path.is_file() && path.starts_with(&root) {
        let stat = path.metadata();
        if stat.as_ref().ok().is_some()
            && max_size.is_some()
            && stat.ok().unwrap().len() > max_size.unwrap() {
            return html(b"I'm afraid, I'm too lazy to serve the requested file. It's pretty big...")
        }

        if filter.as_ref().map_or(false,
                  |f| !f.is_match(path.file_name().unwrap().to_string_lossy().borrow())) {
            return html(b"I don't think you're allowed to do this.");
        }
        let mime: Option<iron::mime::Mime> = path.extension()
            .map(|s| s.to_string_lossy().to_owned())
            .map_or(None, |e| mime_types.get_mime_type(e.borrow()))
            .and_then(|m| m.parse().ok());
        if mime.as_ref().is_some() {
            from_path(&path).map(|r| r.set(Header(ContentType((*mime.as_ref().unwrap()).clone()))))
        } else {
            from_path(path.as_path())
        }
    } else {
        let mut content: Vec<PathBuf> = match fs::read_dir(&path) {
            Ok(s) => s.filter_map(Result::ok).map(|s| s.path()).collect(),
            Err(e) => return html(format!("Error: {}", e.description()).as_bytes())
        };
        content.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
        let mut out = Vec::with_capacity(DEF_LEN);
        render(&mut out, template, root, path, content, filter);
        html(&out)
    }
}

fn main() {
    env_logger::init().ok().expect("Unable to initialise env_logger.");
    let (host, port, threads) = {
        (ARGS.flag_address.clone().unwrap_or(HOST.into()),
         ARGS.flag_port.clone().unwrap_or(PORT),
         ARGS.flag_threads.unwrap_or(num_cpus::get()))
    };

    if ARGS.flag_fork { fork() }

    match Iron::new(serve).listen_with((host.as_ref(), port), threads, iron::Protocol::Http) {
        Ok(_) => (),
        Err(e) => println!("I'm sorry, I failed you.\nError: {:?}", e)
    }
}
