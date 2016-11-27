extern crate hyper;
extern crate rustc_serialize;

static TOKEN : &'static str = "";
static GROUP_ID : isize = 0;
static PICS_DIR : &'static str = "pics";

use hyper::Client;


use std::io::Read;
use std::fs::read_dir;
use std::thread;
use std::time::Duration;

use rustc_serialize::json::Json;

//https://oauth.vk.com/authorize?client_id=5088888&scope=messages,wall,offline&response_type=token
fn vk_req(client: &Client, method: &str, add: &str) -> Json {
    let mut res = client.get(&format!("https://api.vk.com/method/{method}?access_token={token}&{add}",
                                      token = TOKEN,
                                      add = add,
                                      method = method)).send().unwrap();
    let mut body = String::new();
    res.read_to_string(&mut body).unwrap();
    Json::from_str(&body).unwrap()
}

fn ms(client: &Client) {
    let dialogs = vk_req(&Client::new(), "messages.getDialogs", "unread=1&v=5.52");
    let ref dialogs = dialogs.find_path(&vec!["response","items"]).unwrap().as_array().unwrap();
    for d in *dialogs {
        if let None = d.find_path(&vec!["message","chat_id"]) {
            if let Some(body) = d.find_path(&vec!["message","body"]).and_then(|m| m.as_string()) {
                let id = d.find_path(&vec!["message","user_id"]).unwrap().as_u64().unwrap();

                if body.starts_with("Предложка") || body.starts_with("предложка") {
                    let sugg = vk_req(&client, "wall.get", &format!("owner_id={}&filter=suggests", -GROUP_ID));
                    let ref sugg = sugg.find("response")
                        .unwrap()
                        .as_array()
                        .unwrap()[0];
                    vk_req(&client, "messages.send", &format!("user_id={id}&message={ms}",
                                                              id = id,
                                                              ms = sugg));
                } else if body.starts_with("Архив") || body.starts_with("архив") {
                    let count = read_dir(PICS_DIR).unwrap().collect::<Vec<_>>().len();
                    vk_req(&client, "messages.send", &format!("user_id={id}&message={ms}",
                                                              id = id,
                                                              ms = count));
                }
            }
        }
    }
}

fn main() {
    let client = Client::new();
    loop {
        ms(&client);
        thread::sleep(Duration::new(15,0))
    }
}
