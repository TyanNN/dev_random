#![feature(box_syntax)]

extern crate hyper;
extern crate rustc_serialize;
extern crate rand;

static TOKEN: &'static str = "";
static GROUP_ID: isize = 0;

use std::io::Read;
use std::thread;
use std::time::Duration;
use std::fmt::Display;
use std::collections::HashMap;

use hyper::{Client};

use rustc_serialize::json::Json;
use rand::{StdRng,Rng};

macro_rules! map_r(
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m: HashMap<&str, Box<Display>> = ::std::collections::HashMap::new();
            $(
                m.insert($key, $value);
            )+
            m
        }
     };
);

//https://oauth.vk.com/authorize?client_id=5088888&scope=messages,wall,offline,photos&response_type=token
fn vk_req(method_name: &str, additional_params: &HashMap<&str, Box<Display>>) -> Result<Json, hyper::Error> {
    Client::new()
        .get(&format!("https://api.vk.com/method/{method_name}?access_token={token}&{additional_params}",
                      token = TOKEN,
                      additional_params = additional_params.iter().map(|(key,val)| { format!("{key}={val}", key = key, val = val) }).collect::<Vec<_>>().join("&"),
                      method_name = method_name)).send()
        .map(|mut res| {
            let mut body = String::new();
            res.read_to_string(&mut body).unwrap();
            return Json::from_str(&body).unwrap();
        })
}

fn ms() {
    if let Ok(dialogs) = vk_req("messages.getDialogs", &map_r!("unread" => box 1, "v" => box "5.52")).map(|dlgs| dlgs.find_path(&vec!["response","items"]).unwrap().as_array().unwrap().clone()) {
        for d in dialogs {
            if let None = d.find_path(&vec!["message","chat_id"]) {
                if let Some(body) = d.find_path(&vec!["message","body"]).and_then(|m| m.as_string()) {
                    let id = d.find_path(&vec!["message","user_id"]).unwrap().as_u64().unwrap();

                    if body.contains("Предложк") || body.contains("предложк") {
                        if let Ok(suggested_posts) = vk_req("wall.get", &map_r!("owner_id" => box -GROUP_ID, "filter" => box "suggests")) {
                            let tmp = suggested_posts.find("response").unwrap().clone();
                            let sugg = tmp.find("response")
                                .unwrap()
                                .as_array()
                                .unwrap()[0].clone();

                            vk_req("messages.send", &map_r!("id" => box id, "message" => box sugg)).unwrap();

                        }
                    } /*else if body.contains("Архив") || body.contains("архив") {
                        let count = read_dir(PICS_DIR).unwrap().collect::<Vec<_>>().len();
                        vk_req("messages.send", &format!("user_id={id}&message={ms}",
                                                         id = id,
                                                         ms = count));
                    }*/
                }
            }
        }
    }
}

fn post() {
    let suggested_posts = vk_req("wall.get", &map_r!("owner_id" => box -GROUP_ID, "filter" => box "suggests", "v" => box "5.60")).unwrap();
    let suggested_posts = suggested_posts.find("response").unwrap().as_object().unwrap();

    let ref post_count = suggested_posts["count"].as_u64().unwrap();

    if *post_count > 0 {
        let suggested_post = if *post_count > 100 {
            let mut r = Vec::new();
            let count_c = post_count / 100;
            let mut counting = 0;
            while counting < count_c {
                let got = vk_req("wall.get", &map_r!("owner_id" => box -GROUP_ID,
                                                     "filter" => box "suggests",
                                                     "count" => box 100, "offset" => box (counting * 100),
                                                     "v" => box "5.60"));
                let mut got = got.unwrap().find("response").unwrap().as_object().unwrap()["items"].as_array().unwrap().clone();
                r.append(&mut got);
                counting += 1;
            }
            r[StdRng::new().unwrap().gen_range::<usize>(0, r.len() - 1)].clone()
        } else {
            let r = suggested_posts["items"].as_array().unwrap();
            if r.len() == 1 { r[0].clone() } else { r[StdRng::new().unwrap().gen_range::<usize>(0, r.len() - 1)].clone() }
        };

        let suggested_post = suggested_post.as_object().unwrap();
        let post_id = suggested_post["id"].as_u64().unwrap();
        if let Some(atts) = suggested_post.get("attachments") {
            let atts = atts.as_array().unwrap();
            if !atts.iter().filter(|x| {
                let typ = x.as_object().unwrap()["type"].as_string().unwrap();
                if typ == "photo" || typ == "doc" { true } else { false }
            }).collect::<Vec<_>>().is_empty() {
                let (message,signed) = {
                    let t = suggested_post["text"].as_string().unwrap();
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

                vk_req("wall.post", &map_r!("owner_id" => box -GROUP_ID,
                                            "post_id" => box post_id,
                                            "signed" => box signed, "message" => box message, "attachments" => box atts_text)).unwrap();
            } else {
                vk_req("wall.delete", &map_r!("owner_id" => box -GROUP_ID, "post_id" => box post_id)).unwrap();
                post();
            }
        } else {
            vk_req("wall.delete", &map_r!("owner_id" => box -GROUP_ID, "post_id" => box post_id)).unwrap();
            post();
        }
    } else {
        thread::sleep(Duration::new(1,0));

        // let members_count = vk_req("group.getMembers", &map_r!("group_id" => box GROUP_ID)).unwrap().find_path(&vec!["response", "count"]).unwrap().as_i64().unwrap();


        // TODO: Make it work with >= 1000
        let members =  vk_req("groups.getMembers", &map_r!("group_id" => box GROUP_ID)).unwrap().find_path(&vec!["response", "users"]).unwrap().as_array().unwrap().clone();
        let random_member = members[StdRng::new().unwrap().gen_range::<usize>(0, members.len() - 1)].as_i64().unwrap();

        thread::sleep(Duration::new(1,0));

        let albums = vk_req("photos.getAlbums", &map_r!("owner_id" => box random_member, "need_system" => box 1, "v" => box "5.60" )).unwrap();


        if albums.find("error").is_none() {
            let albums = albums.find_path(&vec!["response", "items"]).unwrap().as_array().unwrap();

            let saved_photos_exist = albums.iter().any(|a| a.find("id").unwrap().as_i64().unwrap() == -15 );

            if saved_photos_exist { // There are saved photos, get 'em
                let saved_photos = vk_req("photos.get", &map_r!("owner_id" => box random_member,
                                                                "album_id" => box "saved",
                                                                "v" => box "5.60")).unwrap();
                let saved_photos = saved_photos.find("response").unwrap();
                let saved_photos_count = saved_photos.find("count").unwrap().as_i64().unwrap() as usize;

                let random_photo_id = {
                    let mut r = Vec::new();
                    let count_c = saved_photos_count / 1000;
                    let mut counting = 0;
                    while counting < count_c {
                        let got = vk_req("photos.get", &map_r!("owner_id" => box random_member,
                                                               "album_id" => box "saved",
                                                               "count" => box 1000, "offset" => box (counting * 1000),
                                                               "v" => box "5.60"));
                        let mut got = got.unwrap().find("response").unwrap().as_object().unwrap()["items"].as_array().unwrap().clone();
                        r.append(&mut got);
                        counting += 1;
                    }
                    r[StdRng::new().unwrap().gen_range::<usize>(0, r.len() - 1)].find("id").unwrap().as_i64().unwrap()
                };

                vk_req("wall.post", &map_r!("owner_id" => box -GROUP_ID,
                                            "attachments" => box format!("photo{}_{}", random_member, random_photo_id),
                                            "signed" => box 1)).unwrap();
            } else {
                post();
            }
        }
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
