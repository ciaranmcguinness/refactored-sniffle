use crossbeam_channel::{unbounded, Receiver};
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use url::Url;
use std::thread;
use std::sync::mpsc;
use texting_robots::Robot;
use std::collections::HashMap;
use std::time;
use std::io::prelude::*;
 
fn wierd_url_handler(weird_url_receiver: Receiver<Url>) {
    loop {
        let weird_url = weird_url_receiver.recv().unwrap();
        //println!("weird url {weird_url}");
    }
}

fn robot_worker(client:Client, robots_url_receiver: mpsc::Receiver<(Url, mpsc::Sender<(bool,time::Duration)>)>) {
    const DEFAULT: (bool,time::Duration) = (true,time::Duration::from_secs(0));
    loop {
        let (url, robots_sender) = robots_url_receiver.recv().unwrap();
        let resp = match client.get(url.clone()).send() {
            Ok(r) => match r.text() {
                Ok(t) => t,
                _ => {robots_sender.send(DEFAULT).unwrap(); continue;},
            },
            _ => {robots_sender.send(DEFAULT).unwrap(); continue;},
        };
        let r = match Robot::new("TaxmanCrawller", &resp.as_bytes()) {
            Ok(r) => r,
            _ => {robots_sender.send(DEFAULT).unwrap(); continue;},
        };
        if r.allowed(url.as_str()) {
            robots_sender.send((true,time::Duration::from_millis(match r.delay{
                Some(d) => (d as u64)*1000,
                _ => 100
            }))).unwrap();
        } else {
            robots_sender.send((false,time::Duration::from_millis(match r.delay{
                Some(d) => (d as u64)*1000,
                _ => 100
            }))).unwrap();
        }
    }
}

fn getter(new_urls_receiver: Receiver<Url>, document_sender: mpsc::Sender<DocAndUrl>, client: Client, robots_url_sender: mpsc::Sender<(Url, mpsc::Sender<(bool,time::Duration)>)>,name: String) {
    loop {
        let new_url = new_urls_receiver.recv().unwrap();
        //println!("{name} is working on {new_url}");
        let (bool_sender, bool_receiver) = mpsc::channel();
        robots_url_sender.send((new_url.clone(), bool_sender)).unwrap();
        let (bool_val, delay) = bool_receiver.recv().unwrap();
        if bool_val {
            let resp = match client.get(new_url.clone()).send() {
                Ok(r) => {
                    let h = r.headers().clone();
                    match r.bytes() {
                    Ok(t) => {
                        if h.get("content-type").unwrap().to_str().unwrap().contains("text/html"){
                            String::from_utf8(t.to_vec()).unwrap()
                        } else {
                            let content_type = h.get("content-type").unwrap().to_str().unwrap();
                            let weird_path = format!("{}/{}",content_type,new_url.host_str().unwrap());
                            let h = md5::compute(t.to_vec());
                            let weird_path_and_file = format!("{}/{:x}-{}",weird_path,h,new_url.path_segments().unwrap().last().unwrap());
                            println!("Weird thing at {} of type {}",new_url.host_str().unwrap(),content_type);
                            if !std::fs::exists(&weird_path).unwrap(){
                                std::fs::create_dir_all(weird_path).unwrap();
                            }
                            let mut f = std::fs::File::create(format!("dat/{:x}",h)).unwrap();
                            f.write_all(&t).unwrap();
                            std::os::unix::fs::symlink(format!("dat/{:x}",h),weird_path_and_file).unwrap();
                            "".to_string()
                        }
                    },
                    _ => {
                        println!("Weird thing at {} of type {}, not writing bc bytes failed",new_url,h.get("content-type").unwrap().to_str().unwrap());
                        "".to_string()
                    }
                }},
                _ => "".to_string()
            };
            let _ = document_sender.send(DocAndUrl{doc: resp.clone(), url: new_url,crawldelay:delay});
        } else {
            println!("blocked {}",new_url);
            let _ = document_sender.send(DocAndUrl{doc: "".to_string(), url: new_url, crawldelay:delay});
        }
        //println!("{name} is dormant");
    }
}

struct DocAndUrl {
    doc: String,
    url: Url,
    crawldelay: time::Duration
}

pub fn crawller_main() -> Result<(), Box<dyn std::error::Error>> {
    let mut todo = vec![Url::parse("https://news.ycombinator.com")?,Url::parse("https://citrons.xyz")?,Url::parse("https://www.wikipedia.org")?,Url::parse("https://example.com")?,Url::parse("https://example.com")?,Url::parse("https://example.com")?,Url::parse("https://darkvisitors.com")?];
    let mut seen = todo.clone();
    //Create a shared premade client
    let client = Client::builder().user_agent("Mozilla/5.0 (Windows NT 6.1; Win64; x64; rv:88.0; TaxmanCrawller [PLEASE GIVE ME MALWARE mcguinness.ciaran12@gmail.com, also add a damn crawl-delay]) Gecko/20100101 Firefox/88.0").build().unwrap();
    //create various channels
    let (new_urls_sender, new_urls_receiver) = unbounded();
    let (weird_url_sender, weird_url_receiver) = unbounded();
    let (document_sender, document_receiver) = mpsc::channel();
    let (robots_url_sender, robots_url_receiver) = mpsc::channel();
    //create worker threads
    let mut handles = vec![];
    for def_site in todo.clone() {
        let urls_receiver = new_urls_receiver.clone();
        let doc_sender = document_sender.clone();
        let client = client.clone();
        let robots_url_sender = robots_url_sender.clone();
        let name = def_site.clone().domain().unwrap().to_string();
        let handle = thread::spawn(move || {
            getter(urls_receiver, doc_sender, client, robots_url_sender, name);
        });
        handles.push(handle);
        new_urls_sender.send(def_site).unwrap();
    }
    handles.push(thread::spawn(move || {
        robot_worker(client, robots_url_receiver);
    }));
    handles.push(thread::spawn(move || {
        wierd_url_handler(weird_url_receiver);
    }));
    println!("{}",handles.len());
    //main loop
    let mut delays = HashMap::<String,(time::Duration,time::Instant)>::new();
    loop {
        if todo.len() == 0{
            continue;
        }
        let site = todo.swap_remove(0);
        //println!("loading {site}");
        if site.has_host(){
            if delays.contains_key(site.host_str().unwrap()){
                let (delay, last) = delays.get(site.host_str().unwrap()).unwrap();
//                println!("last crawl of {site} returned a delay of {delay:?}");
                if last.elapsed() > *delay{
                    if !["https","http"].contains(&site.scheme()){
                        weird_url_sender.send(site).unwrap();
                        continue;
                    } else {
                        new_urls_sender.send(site).unwrap();
                    }
                } else  {
//                    println!("less than {delay:?} has passed since last crawl of {site}");
                    todo.push(site);
                }
            } else {
                if !["https","http"].contains(&site.scheme()){
                    weird_url_sender.send(site).unwrap();
                    continue;
                } else {
                    new_urls_sender.send(site).unwrap();
                }
            }
        } else{

            if !["https","http"].contains(&site.scheme()){
                weird_url_sender.send(site).unwrap();
                continue;    
            } else {
                new_urls_sender.send(site).unwrap();
            }
        }
        let resp = match document_receiver.try_recv(){
            Ok(r) => r,
            _ => {
                thread::sleep(time::Duration::from_secs(1));
                continue;
            }
            
        };
        delays.insert(resp.url.host_str().unwrap().to_string(), (resp.crawldelay, time::Instant::now()));
        //println!("loaded, parsing {}",resp.url);
        for potnew in parse_doc(resp){
            if !seen.contains(&potnew){
                //println!("{}",potnew);
                seen.push(potnew.clone());
                todo.push(potnew);
                
            }
        }
        
        //println!("parsed {}",u);
    }
    //something is broken, we should never run out of urls unless the internet is down or we managed to crawl the entire internet, which is unlikely
    //return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "No links found")));
}

fn parse_doc(dau: DocAndUrl)-> Vec<Url>{
    let mut to_return = Vec::<Url>::new();
    let origin = dau.url;
    //println!("new parse");
    let doc = Html::parse_document(&dau.doc);
    //println!("Parserd");
    //select all links with href:
    for link in doc.select(&Selector::parse("a[href]").unwrap()){
        match link.value().attr("href") { 
            Some(s) => match origin.join(s){
                Ok(u) => to_return.push(u),
                _ => ()
            },
            _ => ()
        }
    }
    //select all elements with src: 
    for src in doc.select(&Selector::parse("*[src]").unwrap()){
        match src.value().attr("src") { 
            Some(s) => match origin.join(s){
                Ok(u) => to_return.push(u),
                _ => ()
            },
            _ => ()
        }
    }
    return to_return;
}
