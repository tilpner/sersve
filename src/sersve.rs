#![feature(phase, globs, slicing_syntax, if_let, unboxed_closures)]

extern crate getopts;
extern crate serialize;
extern crate iron;
extern crate persistent;
extern crate error;
extern crate regex;
extern crate "conduit-mime-types" as conduit_mime;
extern crate mime;

#[phase(plugin)]
extern crate lazy_static;

use std::{ str, os };
use std::str::from_str;
use std::path::{ Path, GenericPath };
use std::io::{ fs, Reader };
use std::io::fs::{ File, PathExtensions };
use std::default::Default;
use std::sync::Mutex;

use conduit_mime::Types;

use regex::Regex;

use getopts::{ optopt, optflag, getopts, usage, OptGroup };

use serialize::json;
use serialize::json::Json;

use iron::prelude::*;
use iron::response::modifiers::*;
use iron::status;
use iron::mime::*;
use iron::mime::SubLevel::Ext as SubExt;
use iron::middleware::ChainBuilder;
use iron::typemap::Assoc;

use persistent::Read;

pub mod constants;

#[deriving(Send, Clone, Default, Encodable, Decodable)]
struct Options {
    host: Option<String>,
    port: Option<u16>,
    root: Option<Path>,
    filter: Option<String>,
    max_size: Option<u64>,
}

struct OptCarrier;
impl Assoc<Mutex<Options>> for OptCarrier {}

static UNITS: &'static [&'static str] = &["B", "kB", "MB", "GB", "TB"];
const BRIEF: &'static str = "A minimal directory server, written in Rust with Iron.";

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

fn render(root: Path, dir: Path, files: Vec<Path>, filter: Option<Regex>) -> String {
    fn render_item(url: Path, size: u64, name: &str) -> String {
        format!("<tr><td><a href=\"/{url}\">{name}</a></td><td>{size}</td></tr>\n",
        url = url.display(), size = size_with_unit(size), name = name)
    }

    let mut content = String::new();
    let mut up = dir.clone();
    up.pop();
    if root.is_ancestor_of(&up) {
        content.push_str(render_item(up.path_relative_from(&root).unwrap(), 0, "..")[]);
    }

    for file in files.iter() {
        let relative = file.path_relative_from(&root).unwrap();
        let stat = file.stat().unwrap();
        let filename = file.filename_display().as_maybe_owned().into_string();
        if filter.as_ref().map_or(true, |f| f.is_match(filename[])) {
            content.push_str(render_item(relative, stat.size, filename.clone()[])[]);
        }
    }

    format!("<!DOCTYPE html>
<html>
    <title>{title}</title>
    <style type=\"text/css\">
    {css}
    </style>
    <body>
        <div id=\"container\">
            <h1>{title}</h1>
            <table>
                <thead>
                    <tr>
                        <th>Name</th>
                        <th>Size</th>
                    </tr>
                </thead>
                <tbody>
                {content}
                </tbody>
            </table>
        </div>
    </body>
</html>",
            title = dir.display().as_maybe_owned(),
            css = constants::CSS,
            content = content)
}

fn plain<B: Bodyable>(content: B) -> IronResult<Response> {
    Ok(Response::new()
       .set(Status(status::Ok))
       .set(Body(content)))
}

fn html<B: Bodyable>(content: B) -> IronResult<Response> {
    plain(content).map(|r| r.set(ContentType(Mime(Text, Html, Vec::new()))))
}

fn binary<B: Bodyable>(content: B) -> IronResult<Response> {
    plain(content).map(
        |r| r.set(ContentType(Mime(Application, SubExt("octet-stream".into_string()), Vec::new()))))
}

fn guess_text(data: &[u8]) -> bool {
    let mut total = 0u;
    let mut text = 0u;
    for (c, _) in data.iter().zip(range(0u, 1000)) {
        let c = *c as char;
        if c.is_alphanumeric() || c.is_whitespace() {
            text += 1;
        }
        total += 1;
    }
    text as f64 / total as f64 > 0.75
}

fn serve(req: &mut Request) -> IronResult<Response> {
    let (root, filter_str, max_size) = {
        let o = req.get::<Read<OptCarrier, Mutex<Options>>>().unwrap();
        let mutex = o.lock();
        (mutex.root.clone().unwrap_or_else(|| os::getcwd().ok().unwrap()),
         mutex.filter.clone(),
         mutex.max_size)
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
        //if guess_text(content[]) { plain(content[]) } else { binary(content[]) }
    } else {
        let mut content = match fs::readdir(&path) {
            Ok(s) => s,
            Err(e) => return html(e.desc)
        };
        content.sort_by(|a, b| a.filename_str().unwrap().cmp(b.filename_str().unwrap()));
        html(render(root, path, content, filter)[])
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

    let options: Mutex<Options> = Mutex::new(Default::default());

    matches.opt_str("c").map(|conf_file| {
        let conf_file = File::open(&Path::new(conf_file));
        conf_file.as_ref().map_err::<()>(|e| panic!("{}", e.desc)).unwrap();
        let json = match json::from_reader(&mut conf_file.ok().unwrap()) {
            Ok(Json::Object(o)) => o,
            _ => panic!("Invalid configuration file. Doesn't contain top-level object.")
        };
        let mut o = options.lock();
        o.host = match json.get("address") {
            Some(&Json::String(ref s)) => Some((*s).clone()),
            None => None,
            _ => panic!("Invalid configuration file. `address` field must be a string.")
        };
        o.port = match json.get("port") {
            Some(&Json::U64(u)) => Some(u as u16),
            None => None,
            _ => panic!("Invalid configuration file. `port` field must be an unsigned integer.")
        };
        o.root = match json.get("root") {
            Some(&Json::String(ref s)) => Some(Path::new((*s).clone())),
            None => None,
            _ => panic!("Invalid configuration file. `root` field must be a string.")
        };
        o.filter = match json.get("filter") {
            Some(&Json::String(ref s)) => Some((*s).clone()),
            None => None,
            _ => panic!("Invalid configuration file. `filter` field must be a string.")
        };
        o.max_size = match json.get("size") {
            Some(&Json::U64(u)) => Some(u),
            None => None,
            _ => panic!("Invalid configuration file. `size` field must be an unsigned integer.")
        }
    });

    let (host, port) = {
        let mut o = options.lock();
        o.host = o.host.clone().or(matches.opt_str("a"));
        o.port = o.port.or(matches.opt_str("p").and_then(|p| str::from_str(p[])));
        o.root = o.root.clone().or(matches.opt_str("r").and_then(|p| Path::new_opt(p)));
        o.filter = o.filter.clone().or(matches.opt_str("f"));
        o.max_size = o.max_size.or(matches.opt_str("s").and_then(|s| str::from_str(s[])));
        (o.host.clone().unwrap_or("0.0.0.0".into_string()),
         o.port.clone().unwrap_or(8080))
    };

    let mut chain = ChainBuilder::new(serve);
    chain.link(Read::<OptCarrier, Mutex<Options>>::both(options));
    match Iron::new(chain).listen((host[], port)) {
        Ok(_) => (),
        Err(e) => println!("I'm sorry, I failed you. {}", e)
    }
}
