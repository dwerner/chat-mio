use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::time::{SystemTime, UNIX_EPOCH};

use super::messages::{Chat, Message};

use lazy_static::lazy_static;

pub(crate) fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

lazy_static! {
    static ref USERS: HashMap<u64, Vec<u64>> = {
        let file = File::open("contacts.json").expect("unable to open contacts.json");
        let reader = BufReader::new(file);
        let m: HashMap<_, _> = serde_json::from_reader(reader).unwrap();
        m
    };
}

pub struct ChatRoom {
    chat: Chat,
    log: BinaryHeap<Message>,
}

impl ChatRoom {
    pub fn new(chat: Chat) -> Self {
        ChatRoom {
            chat,
            log: BinaryHeap::new(),
        }
    }
}

#[derive(Default)]
pub struct ChatService {
    chats: HashMap<(u64, u64), ChatRoom>,
    chat_keys: HashMap<u64, (u64, u64)>,
}

impl ChatService {
    /// Adds a new chat - user a and b must have each other in their contact lists
    pub fn add_chat(&mut self, chat: Chat) -> Result<(), Box<dyn Error>> {
        let user_a = chat.participant_ids[0];
        let user_b = chat.participant_ids[1];

        if self.chats.contains_key(&(user_a, user_b)) {
            return Err("Chat already exists".into());
        }
        let a = USERS.get(&user_a);
        let b = USERS.get(&user_b);
        if let (Some(a), Some(b)) = (a, b) {
            if a.contains(&user_b) && b.contains(&user_a) {
                self.chat_keys.insert(chat.id, (user_a, user_b));
                let chatroom = ChatRoom::new(chat);
                self.chats.insert((user_a, user_b), chatroom);
                return Ok(());
            } else {
                if !a.contains(&user_b) {
                    return Err(format!(
                        "user {} does not have user {} in their contact list.",
                        user_a, user_b
                    )
                    .into());
                }
                if !b.contains(&user_a) {
                    return Err(format!(
                        "user {} does not have user {} in their contact list.",
                        user_b, user_a
                    )
                    .into());
                }
            }
        }
        Err("unable to create chat".into())
    }

    pub fn send_message(&mut self, chat_id: u64, message: Message) -> Result<(), Box<dyn Error>> {
        let key = match self.chat_keys.get(&chat_id) {
            Some(key) => key,
            None => return Err(format!("Unable to find chat with id {}", chat_id).into()),
        };
        match self.chats.get_mut(&key) {
            Some(chat) => {
                println!(
                    "adding message to log for chat id {} users {:?}",
                    chat_id, key
                );
                chat.log.push(message);
            }
            None => return Err(format!("Unable to find chat for {:?}", key).into()),
        }
        Ok(())
    }

    pub fn get_messages(&self, chat_id: u64) -> Result<Vec<Message>, Box<dyn Error>> {
        println!("GET MESSAGES");
        let key = match self.chat_keys.get(&chat_id) {
            Some(key) => key,
            None => return Err("Unable to find key".into()),
        };
        let chat = match self.chats.get(&key) {
            Some(chat) => chat,
            None => return Err("Unable to find chatroom".into()),
        };
        let current_log = chat.log.clone();
        println!("chat log : {:?}", chat.log);
        Ok(current_log.into_sorted_vec())
    }

    pub fn get_user_chats(&self, user_id: u64) -> Vec<&Chat> {
        self.chats
            .iter()
            .filter(|((a, b), _)| *a == user_id || *b == user_id)
            .map(|(_, v)| &v.chat)
            .collect::<Vec<_>>()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn msg(src: u64, dst: u64) -> Message {
        let ts = timestamp();
        Message {
            id: String::new(),
            timestamp: ts,
            source_user_id: src,
            destination_user_id: dst,
            message: format!("{} to {} at {}", src, dst, ts),
        }
    }

    #[test]
    fn test_chat_service() {
        let mut service = ChatService::default();

        // adding message to log for chat id 11872 users (58534, 74827)
        let chat = Chat {
            id: 11872,
            participant_ids: [58534, 74827],
        };

        service.add_chat(chat).unwrap();

        for i in 0..10 {
            service.send_message(11872, msg(58534, 74827)).unwrap();
        }

        let messages = service.get_messages(11872).unwrap();
        assert_eq!(messages.len(), 10);

        let mut high_mark = 0;
        for msg in messages {
            if high_mark == 0 {
                high_mark = msg.timestamp;
            }

            assert!(high_mark >= msg.timestamp);
        }
    }
}
