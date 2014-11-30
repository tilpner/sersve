#![feature(phase, globs, slicing_syntax, if_let, unboxed_closures)]

extern crate getopts;
extern crate serialize;
extern crate iron;
extern crate persistent;
extern crate error;
extern crate regex;
extern crate "conduit-mime-types" as conduit_mime;
extern crate mime;
extern crate mustache;

#[phase(plugin)]
extern crate lazy_static;

use std::{ str, os };
use std::str::from_str;
use std::path::{ Path, GenericPath };
use std::io::{ fs, Reader };
use std::io::fs::{ File, PathExtensions };
use std::default::Default;
use std::sync::{ Arc };

use regex::Regex;

use conduit_mime::Types;

use getopts::{ optopt, optflag, getopts, usage, OptGroup };

use serialize::json;
use serialize::json::Json;

use iron::prelude::*;
use iron::response::modifiers::*;
use iron::status;
use iron::mime::*;
use iron::middleware::ChainBuilder;
use iron::typemap::Assoc;

use persistent::Read;

use mustache::{ Template, VecBuilder, MapBuilder };

pub mod constants;

#[deriving(Send, Clone, Default, Encodable, Decodable)]
struct Options {
    host: Option<String>,
    port: Option<u16>,
    root: Option<Path>,
    filter: Option<String>,
    max_size: Option<u64>,
    template: Option<String>
}

struct OptCarrier;
impl Assoc<Arc<Options>> for OptCarrier {}

#[deriving(Send, Clone)]
struct State {
    template: Template
}

struct StateCarrier;
impl Assoc<Arc<State>> for StateCarrier {}

static UNITS: &'static [&'static str] = &["B", "kB", "MB", "GB", "TB"];
const BRIEF: &'static str = "A minimal directory server, written in Rust with Iron.";

const KEY_TITLE: &'static str = "title";
const KEY_CONTENT: &'static str = "content";
const KEY_URL: &'static str = "url";
const KEY_SIZE: &'static str = "size";
const KEY_NAME: &'static str = "name";

lazy_static! {
    static ref MIME: Types = Types::new().ok().unwrap();
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

fn render(template: Template, root: Path, dir: Path, files: Vec<Path>, filter: Option<Regex>) -> String {
    let data = MapBuilder::new()
        .insert_str(KEY_TITLE, dir.display().as_cow().into_owned())
        .insert_vec(KEY_CONTENT, |mut vec: VecBuilder| {
            let item = |map: MapBuilder, url: &Path, size: u64, name: String| {
                map.insert(KEY_URL, &format!("{}", url.display())[]).unwrap()
                   .insert(KEY_SIZE, &size_with_unit(size)[]).unwrap()
                   .insert_str(KEY_NAME, name)
            };
            let mut up = dir.clone();
            up.pop();
            if root.is_ancestor_of(&up) {
                vec = vec.push_map(|map: MapBuilder| item(map, &up.path_relative_from(&root).unwrap(), 0, "..".into_string()));
            }

            for file in files.iter() {
                let relative = file.path_relative_from(&root).unwrap();
                let stat = file.stat().unwrap();
                let filename = file.filename_display().as_cow().into_owned();
                if filter.as_ref().map_or(true, |f| f.is_match(filename[])) {
                    vec = vec.push_map(|map| item(map, &relative, stat.size, filename.clone()));
                }
            }
            unsafe { std::mem::transmute(vec) }
        }).build();

    let mut out = Vec::new(); // with_capacity(template.len())
    template.render_data(&mut out, &data);
    unsafe { String::from_utf8_unchecked(out) }
}

fn plain<B: Bodyable>(content: B) -> IronResult<Response> {
    Ok(Response::new()
       .set(Status(status::Ok))
       .set(Body(content)))
}

fn html<B: Bodyable>(content: B) -> IronResult<Response> {
    plain(content).map(|r| r.set(ContentType(Mime(Text, Html, Vec::new()))))
}

fn serve(req: &mut Request) -> IronResult<Response> {
    let (root, filter_str, max_size) = {
        let o = req.get::<Read<OptCarrier, Arc<Options>>>().unwrap();
        (o.root.clone().unwrap_or_else(|| os::getcwd().ok().unwrap()),
         o.filter.clone(),
         o.max_size)
    };

    let template = {
        let s = req.get::<Read<StateCarrier, Arc<State>>>().unwrap();
        s.template.clone()
    };

    let mut path = root.clone();
    for part in req.url.path.iter() { path.push(part[]) }
    if !path.exists() { return html("Well, no... We don't have that today."); }

    let filter = filter_str.and_then(|s| Regex::new(s[]).ok());

    if path.is_file() && root.is_ancestor_of(&path) {
        let stat = path.stat();
        if stat.as_ref().ok().is_some() && max_size.is_some() && stat.ok().unwrap().size > max_size.unwrap() {
            return html("I'm afraid, I'm too lazy to serve the requested file. It's pretty big...")
        }
        let content = match File::open(&path).read_to_end() {
            Ok(s) => s,
            Err(e) => return html(e.desc)
        };

        if filter.as_ref().map_or(false, |f| !f.is_match(path.filename_str().unwrap())) {
            return html("I don't think you're allowed to do this.");
        }
        let mime: Option<iron::mime::Mime> = path.extension_str()
            .map_or(None, |e| MIME.get_mime_type(e))
            .map_or(None, |m| from_str(m));
        if mime.as_ref().is_some() {
            plain(content[]).map(|r| r.set(ContentType((*mime.as_ref().unwrap()).clone())))
        } else {
            plain(content[])
        }
    } else {
        let mut content = match fs::readdir(&path) {
            Ok(s) => s,
            Err(e) => return html(e.desc)
        };
        content.sort_by(|a, b| a.filename_str().unwrap().cmp(b.filename_str().unwrap()));
        html(render(template, root, path, content, filter)[])
    }
}

fn print_usage(program: &str, opts: &[OptGroup]) {
    println!("Usage: {} [options]\n", program);
    println!("{}", usage(BRIEF, opts));
}

fn main() {
    let args: Vec<String> = os::args();
    let program = args[0].clone();
    let opts = &[
        optopt("c", "config", "set config file name", "NAME"),
        optopt("a", "address", "the address to bind to", "HOST"),
        optopt("p", "port", "the port to serve", "PORT"),
        optopt("r", "root", "the uppermost directory to serve", "ROOT"),
        optopt("f", "filter", "a regular expression to filter the filenames", "REGEX"),
        optopt("s", "size", "the maximum size of a file that will be served", "BYTES"),
        optopt("t", "template", "a mustache template to use for rendering", "TEMPLATE"),
        optflag("h", "help", "print this help menu")
    ];
    let matches = match getopts(args.tail(), opts) {
        Ok(m) => { m }
        Err(f) => {
            println!("{}", f.to_string());
            return;
        }
    };

    if matches.opt_present("h") {
        print_usage(program[], opts);
        return;
    }

    let mut options: Options = Default::default();

    matches.opt_str("c").map(|conf_file| {
        let conf_file = File::open(&Path::new(conf_file));
        conf_file.as_ref().map_err::<()>(|e| panic!("{}", e.desc)).unwrap();

        // cannot if-let, because typesafe errors are helpful
        let json = match json::from_reader(&mut conf_file.ok().unwrap()) {
            Ok(Json::Object(o)) => o,
            _ => panic!("Invalid configuration file. Doesn't contain top-level object.")
        };
        options.host = match json.get("address") {
            Some(&Json::String(ref s)) => Some((*s).clone()),
            None => None,
            _ => panic!("Invalid configuration file. `address` field must be a string.")
        };
        options.port = match json.get("port") {
            Some(&Json::U64(u)) => Some(u as u16),
            None => None,
            _ => panic!("Invalid configuration file. `port` field must be an unsigned integer.")
        };
        options.root = match json.get("root") {
            Some(&Json::String(ref s)) => Some(Path::new((*s).clone())),
            None => None,
            _ => panic!("Invalid configuration file. `root` field must be a string.")
        };
        options.filter = match json.get("filter") {
            Some(&Json::String(ref s)) => Some((*s).clone()),
            None => None,
            _ => panic!("Invalid configuration file. `filter` field must be a string.")
        };
        options.max_size = match json.get("size") {
            Some(&Json::U64(u)) => Some(u),
            None => None,
            _ => panic!("Invalid configuration file. `size` field must be an unsigned integer.")
        };
        options.template = match json.get("template") {
            Some(&Json::String(ref s)) => Some((*s).clone()),
            None => None,
            _ => panic!("Invalid configuration file. `template` field must be a string.")
        };
    });

    let (host, port) = {
        options.host = matches.opt_str("a").or(options.host);
        options.port = matches.opt_str("p").and_then(|p| str::from_str(p[])).or(options.port);
        options.root = matches.opt_str("r").and_then(|p| Path::new_opt(p)).or(options.root);
        options.filter = matches.opt_str("f").or(options.filter);
        options.max_size = matches.opt_str("s").and_then(|s| str::from_str(s[])).or(options.max_size);
        options.template = matches.opt_str("t").or(options.template);
        (options.host.clone().unwrap_or("0.0.0.0".into_string()),
         options.port.clone().unwrap_or(8080))
    };

    let template = mustache::compile_str(options.template.clone().unwrap_or(constants::OPT_TEMPLATE.into_string())[]);
    let state = State {
        template: template
    };

    let mut chain = ChainBuilder::new(serve);
    chain.link(Read::<OptCarrier, Arc<Options>>::both(Arc::new(options)));
    chain.link(Read::<StateCarrier, Arc<State>>::both(Arc::new(state)));
    match Iron::new(chain).listen((host[], port)) {
        Ok(_) => (),
        Err(e) => println!("I'm sorry, I failed you. {}", e)
    }
}
