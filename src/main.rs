extern crate hyper;
extern crate rustc_serialize;
extern crate rand;
extern crate multipart;

static TOKEN : &'static str = "";
static GROUP_ID : isize = 0;
static PICS_DIR : &'static str = "pics";

use std::io::Read;
use std::fs::{read_dir,remove_file};
use std::thread;
use std::time::Duration;
use std::str::FromStr;

use hyper::{Client};
use hyper::client::request::Request;

use rustc_serialize::json::Json;
use multipart::client::Multipart;
use rand::{StdRng,Rng};

//https://oauth.vk.com/authorize?client_id=5088888&scope=messages,wall,offline,photos&response_type=token
fn vk_req(method: &str, add: &str) -> Json {
    let client = Client::new();
    let mut res = client.get(&format!("https://api.vk.com/method/{method}?access_token={token}&{add}",
                                      token = TOKEN,
                                      add = add,
                                      method = method)).send().unwrap();
    let mut body = String::new();
    res.read_to_string(&mut body).unwrap();
    Json::from_str(&body).unwrap()
}

fn ms() {
    let dialogs = vk_req("messages.getDialogs", "unread=1&v=5.52");
    let ref dialogs = dialogs.find_path(&vec!["response","items"]).unwrap().as_array().unwrap();
    for d in *dialogs {
        if let None = d.find_path(&vec!["message","chat_id"]) {
            if let Some(body) = d.find_path(&vec!["message","body"]).and_then(|m| m.as_string()) {
                let id = d.find_path(&vec!["message","user_id"]).unwrap().as_u64().unwrap();

                if body.contains("Предложк") || body.contains("предложк") {
                    let sugg = vk_req("wall.get", &format!("owner_id={}&filter=suggests", -GROUP_ID));
                    let ref sugg = sugg.find("response")
                        .unwrap()
                        .as_array()
                        .unwrap()[0];
                    vk_req("messages.send", &format!("user_id={id}&message={ms}",
                                                              id = id,
                                                              ms = sugg));
                } else if body.contains("Архив") || body.contains("архив") {
                    let count = read_dir(PICS_DIR).unwrap().collect::<Vec<_>>().len();
                    vk_req("messages.send", &format!("user_id={id}&message={ms}",
                                                              id = id,
                                                              ms = count));
                }
            }
        }
    }
}

fn post() {
    let sugg = vk_req("wall.get", &format!("owner_id={}&filter=suggests&v=5.60", -GROUP_ID));
    let sugg = sugg.find("response").unwrap().as_object().unwrap();

    let ref sugg_count = sugg["count"].as_u64().unwrap();
    if *sugg_count > 0 {
        let sugg = if *sugg_count > 100 {
            let mut r = Vec::new();
            let count_c = sugg_count / 100;
            let mut counting = 0;
            while counting < count_c {
                let got = vk_req("wall.get", &format!("owner_id={}&filter=suggests&count=100&offset={}&v=5.60", -GROUP_ID, counting * 100));
                let mut got = got.find("response").unwrap().as_object().unwrap()["items"].as_array().unwrap().clone();
                r.append(&mut got);
                counting += 1;
            }
            r[StdRng::new().unwrap().gen_range::<usize>(0, r.len() - 1)].clone()
        } else {
             let r = sugg["items"].as_array().unwrap();
             if r.len() == 1 { r[0].clone() } else { r[StdRng::new().unwrap().gen_range::<usize>(0, r.len() - 1)].clone() }
        };
        let sugg = sugg.as_object().unwrap();
        let post_id = sugg["id"].as_u64().unwrap();
        if let Some(atts) = sugg.get("attachments") {
            let atts = atts.as_array().unwrap();
            if !atts.iter().filter(|x| {
                let typ = x.as_object().unwrap()["type"].as_string().unwrap();
                if typ == "photo" || typ == "doc" { true } else { false }
            }).collect::<Vec<_>>().is_empty() {
                let (message,signed) = {
                    let t = sugg["text"].as_string().unwrap();
                    let signed = if t.contains("анон") || t.contains("Анон") { 0 } else { 1 };
                    let t = t.replace("анон","").replace("Анон","");
                    (t, signed)
                };

                let atts_text = atts.iter().map(|x| {
                    let x = x.as_object().unwrap();
                    let typ = x["type"].as_string().unwrap();
                    let info = x[typ].as_object().unwrap();
                    let owner = info["owner_id"].as_i64().unwrap();
                    let id = info["id"].as_i64().unwrap();
                    format!("{}{}_{}", typ, owner, id)
                }).collect::<Vec<_>>().join(",");

                vk_req("wall.post", &format!("owner_id={}&post_id={}&signed={}&message={}&attachments={}", -GROUP_ID, post_id, signed, message,atts_text));
            } else {
                vk_req("wall.delete", &format!("owner_id={}&post_id={}", -GROUP_ID, post_id));
                post();
            }
        } else {
            vk_req("wall.delete", &format!("owner_id={}&post_id={}", -GROUP_ID, post_id));
            post();
        }
    } else {
        let upload_url = vk_req("photos.getWallUploadServer", &format!("group_id={}", GROUP_ID));
        let upload_url = upload_url.find_path(&vec!["response","upload_url"])
            .unwrap().as_string().unwrap();
        let pic = read_dir(PICS_DIR).unwrap().map(|x| x.unwrap()).collect::<Vec<_>>();
        let pic = if pic.len() > 1 {
            &pic[StdRng::new().unwrap().gen_range::<usize>(0, pic.len() - 1)]
        } else {
            &pic[0]
        };

        let request = Request::new(
            hyper::method::Method::Post,
            hyper::Url::from_str(&upload_url).unwrap()
        ).unwrap();
        let mut request = Multipart::from_request(request).unwrap();
        request.write_file("photo", pic.path()).unwrap();
        let mut res = request.send().unwrap();
        let mut res_s = String::new();
        res.read_to_string(&mut res_s).unwrap();
        let res = Json::from_str(&res_s).unwrap();
        let photo = res.as_object().unwrap();
        let ref photo = vk_req("photos.saveWallPhoto", &format!("group_id={gid}&server={serv}&photo={photo}&hash={hash}",
                                                           gid = GROUP_ID,
                                                           serv = photo["server"].as_u64().unwrap(),
                                                           photo = photo["photo"].as_string().unwrap(),
                                                           hash = photo["hash"].as_string().unwrap()));
        let photo = photo.as_object().unwrap()["response"].as_array().unwrap()[0].as_object().unwrap();
        vk_req("wall.post", &format!("owner_id={}&attachments={}&signed=1", -GROUP_ID, photo["id"].as_string().unwrap()));
        remove_file(pic.path()).unwrap();
    }
}

fn main() {
    if std::env::args().collect::<Vec<_>>().contains(&"post".to_string()) {
        post()
    } else {
        loop {
            ms();
            thread::sleep(Duration::new(15,0))
        }
    }
}
