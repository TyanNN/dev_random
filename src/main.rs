#![feature(box_syntax)]

extern crate hyper;
extern crate rustc_serialize;
extern crate rand;
#[macro_use] extern crate log;
extern crate simplelog;
extern crate dotenv;

use std::env;
use std::io::Read;
use std::thread;
use std::time::Duration;
use std::fmt::Display;
use std::collections::HashMap;

use simplelog::*;
use std::fs::OpenOptions;

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
fn vk_req(token: &str, method_name: &str, additional_params: &HashMap<&str, Box<Display>>) -> Result<Json, hyper::Error> {
    Client::new()
        .get(&format!("https://api.vk.com/method/{method_name}?access_token={token}&{additional_params}",
                      token = token,
                      additional_params = additional_params.iter().map(|(key,val)| { format!("{key}={val}", key = key, val = val) }).collect::<Vec<_>>().join("&"),
                      method_name = method_name)).send()
        .map(|mut res| {
            let mut body = String::new();
            res.read_to_string(&mut body).unwrap();
            return Json::from_str(&body).unwrap();
        })
}

fn ms(token: &str, group_id: isize) {
    if let Ok(dialogs) = vk_req(token, "messages.getDialogs", &map_r!("unread" => box 1, "v" => box "5.52")).map(|dlgs| dlgs.find_path(&vec!["response","items"]).unwrap().as_array().unwrap().clone()) {
        for d in dialogs {
            if let None = d.find_path(&vec!["message","chat_id"]) {
                if let Some(body) = d.find_path(&vec!["message","body"]).and_then(|m| m.as_string()) {
                    let id = d.find_path(&vec!["message","user_id"]).unwrap().as_u64().unwrap();

                    if body.contains("Предложк") || body.contains("предложк") {
                        if let Ok(suggested_posts) = vk_req(token, "wall.get", &map_r!("owner_id" => box -group_id, "filter" => box "suggests")) {
                            let tmp = suggested_posts.find("response").unwrap().clone();
                            let sugg = tmp.find("response")
                                .unwrap()
                                .as_array()
                                .unwrap()[0].clone();

                            vk_req(token, "messages.send", &map_r!("id" => box id, "message" => box sugg)).unwrap();

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

fn post(token: &str, group_id: isize) {
    info!("Собираю посты из предложки");
    let suggested_posts = vk_req(token, "wall.get", &map_r!("owner_id" => box -group_id, "filter" => box "suggests", "v" => box "5.60")).unwrap();
    let suggested_posts = suggested_posts.find("response").unwrap().as_object().unwrap();

    let ref post_count = suggested_posts["count"].as_u64().unwrap();

    if *post_count > 0 {
        info!("В предложке {} постов", post_count);

        let suggested_post = if *post_count > 100 {
            debug!("Больше 100 постов, начинаю собирать остальные");

            let mut r = Vec::new();
            let count_c = post_count / 100;
            let mut counting = 0;
            while counting < count_c {
                let got = vk_req(token, "wall.get", &map_r!("owner_id" => box -group_id,
                                                            "filter" => box "suggests",
                                                            "count" => box 100, "offset" => box (counting * 100),
                                                            "v" => box "5.60"));
                let mut got = got.unwrap().find("response").unwrap().as_object().unwrap()["items"].as_array().unwrap().clone();
                r.append(&mut got);

                debug!("Собрано {}/{} страниц постов", counting, count_c);

                counting += 1;
            }
            r[StdRng::new().unwrap().gen_range::<usize>(0, r.len() - 1)].clone()
        } else {
            let r = suggested_posts["items"].as_array().unwrap();
            if r.len() == 1 { r[0].clone() } else { r[StdRng::new().unwrap().gen_range::<usize>(0, r.len() - 1)].clone() }
        };

        let suggested_post = suggested_post.as_object().unwrap();
        let post_id = suggested_post["id"].as_u64().unwrap();

        info!("Выбран пост #{}", post_id);

        if let Some(atts) = suggested_post.get("attachments") {
            let atts = atts.as_array().unwrap();

            info!("У поста есть прикреплённые вещи ({})", atts.len());

            if !atts.iter().filter(|x| {
                let typ = x.as_object().unwrap()["type"].as_string().unwrap();
                (typ == "photo" || typ == "doc")
            }).collect::<Vec<_>>().is_empty() {
                info!("У поста есть прикреплённые картинки");

                let (message,signed) = {
                    let t = suggested_post["text"].as_string().unwrap();
                    let signed = if t.contains("анон") || t.contains("Анон") { 0 } else { 1 };
                    let t = t.replace("анон","").replace("Анон","");
                    (t, signed)
                };

                if signed == 0 {
                    info!("Постим анонимно");
                }

                let atts_text = atts.iter().map(|x| {
                    let x = x.as_object().unwrap();
                    let typ = x["type"].as_string().unwrap();
                    let info = x[typ].as_object().unwrap();
                    let owner = info["owner_id"].as_i64().unwrap();
                    let id = info["id"].as_i64().unwrap();
                    format!("{}{}_{}", typ, owner, id)
                }).collect::<Vec<_>>().join(",");

                debug!("Итоговая строка прикреплённых вещей: {}", atts_text);

                let res = vk_req(token, "wall.post", &map_r!("owner_id" => box -group_id,
                                                             "post_id" => box post_id,
                                                             "signed" => box signed, "message" => box message,
                                                             "attachments" => box atts_text, "v" => box "5.60"));
                match res {
                    Ok(ref res) if res.find("error").is_none() => info!("Пост успешно отправлен, ID поста: {}", res.find_path(&vec!["response", "post_id"]).unwrap().as_i64().unwrap()),
                    Ok(ref res) => error!("Запрос на добавление дошёл успешно, но верунлась ошибка: {:?}", res),
                    Err(e) => error!("Не могу добавить пост, ошибка: {:?}", e)
                }
            } else {
                info!("В посте не было прикреплённых картинок, удаляю пост");
                vk_req(token, "wall.delete", &map_r!("owner_id" => box -group_id, "post_id" => box post_id)).unwrap();

                info!("Перезапускаю проверку предложки");
                post(token, group_id);
            }
        } else {
            info!("В посте не было прикреплённых вещей, удаляю пост");
            vk_req(token, "wall.delete", &map_r!("owner_id" => box -group_id, "post_id" => box post_id)).unwrap();

            info!("Перезапускаю проверку предложки");
            post(token, group_id);
        }
    } else {
        thread::sleep(Duration::new(1,0));

        info!("В предложке нет постов! Смотрю сохранёнки подписчиков");

        debug!("FIXME: заставить работать с >1000 подписчиками");
        // let members_count = vk_req("group.getMembers", &map_r!("group_id" => box GROUP_ID)).unwrap().find_path(&vec!["response", "count"]).unwrap().as_i64().unwrap();

        let members =  vk_req(token, "groups.getMembers", &map_r!("group_id" => box group_id)).unwrap().find_path(&vec!["response", "users"]).unwrap().as_array().unwrap().clone();
        let random_member = members[StdRng::new().unwrap().gen_range::<usize>(0, members.len() - 1)].as_i64().unwrap();

        info!("ID случайного подписчика: {}", random_member);

        thread::sleep(Duration::new(1,0));

        info!("Смотрю альбомы");

        let albums = vk_req(token, "photos.getAlbums", &map_r!("owner_id" => box random_member, "need_system" => box 1, "v" => box "5.60" )).unwrap();

        if albums.find("error").is_none() {
            let albums = albums.find_path(&vec!["response", "items"]).unwrap().as_array().unwrap();

            let saved_photos_exist = albums.iter().any(|a| a.find("id").unwrap().as_i64().unwrap() == -15 );

            if saved_photos_exist { // There are saved photos, get 'em
                info!("Альбом сохранённых фото найден");

                let saved_photos = vk_req(token, "photos.get", &map_r!("owner_id" => box random_member,
                                                                       "album_id" => box "saved",
                                                                       "v" => box "5.60")).unwrap();
                let saved_photos = saved_photos.find("response").unwrap();
                let saved_photos_count = saved_photos.find("count").unwrap().as_i64().unwrap() as usize;

                if saved_photos_count > 0 {
                    info!("Всего сохранённых фото: {}", saved_photos_count);

                    let random_photo_id = if saved_photos_count > 1000 {
                        debug!("Сохранённых фото больше 1000, запускаю сборку по страницам");

                        let mut r = Vec::new();
                        let count_c = saved_photos_count / 1000;
                        let mut counting = 0;
                        while counting < count_c {
                            let got = vk_req(token, "photos.get", &map_r!("owner_id" => box random_member,
                                                                          "album_id" => box "saved",
                                                                          "count" => box 1000, "offset" => box (counting * 1000),
                                                                          "v" => box "5.60"));
                            let mut got = got.unwrap().find("response").unwrap().as_object().unwrap()["items"].as_array().unwrap().clone();
                            r.append(&mut got);

                            debug!("Собрано {}/{} страниц фото", counting, count_c);

                            counting += 1;
                        }
                        r[StdRng::new().unwrap().gen_range::<usize>(0, r.len() - 1)].find("id").unwrap().as_i64().unwrap()
                    } else {
                        let saved_photos_array = saved_photos.find("items").unwrap().as_array().unwrap();

                        saved_photos_array[StdRng::new().unwrap().gen_range::<usize>(0, saved_photos_array.len() - 1)].find("id").unwrap().as_i64().unwrap()
                    };

                    info!("ID случайного фото: {}", random_photo_id);

                    let res = vk_req(token, "wall.post", &map_r!("owner_id" => box -group_id,
                                                                 "attachments" => box format!("photo{}_{}", random_member, random_photo_id),
                                                                 "signed" => box 1));
                    match res {
                        Ok(ref res) if res.find("error").is_none() => info!("Пост успешно отправлен, ID поста: {}", res.find_path(&vec!["response", "post_id"]).unwrap().as_i64().unwrap()),
                        Ok(ref res) => error!("Запрос на добавление дошёл успешно, но верунлась ошибка: {:?}", res),
                        Err(e) => error!("Не могу добавить пост, ошибка: {:?}", e)
                    }
                } else {
                    info!("У пользователя 0 сохранённых фото, перезапускаю проверку предложки");

                    post(token, group_id);
                }
            } else {
                info!("Нет альбома с сохранёнными фото, перзеапускаю проверку предложки");

                post(token, group_id);
            }
        } else {
            error!("Ошибка при получении альбомов: {:?}", albums);
            info!("Перезапускаю проверку предложки");

            post(token, group_id);
        }
    }
}

fn main() {
    dotenv::dotenv().ok();

    let log_file = env::var("DEVRANDOM_LOG_FILE").unwrap_or("dev_random.log".to_string());
    let group_id = env::var("DEVRANDOM_GROUP_ID")
        .expect("DEVRANDOM_GROUP_ID is not set")
        .parse::<isize>().expect("DEVRANDOM_GROUP_ID is not a number");
    let token= env::var("DEVRANDOM_TOKEN").expect("DEVRANDOM_TOKEN is not set");

    WriteLogger::init(LogLevelFilter::Info, Config::default(), OpenOptions::new().append(true).create(true).open(log_file).unwrap()).unwrap();

    if std::env::args().collect::<Vec<_>>().contains(&"post".to_string()) {
        post(&token, group_id);
    } else {
        loop {
            ms(&token, group_id);
            thread::sleep(Duration::new(15,0))
        }
    }
}
