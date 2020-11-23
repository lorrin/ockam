pub mod ockam_redis {
    use crate::topic::*;
    use simple_redis::{Message, Interrupts};
    use simple_redis::client::Client;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::borrow::Borrow;

    struct RedisManager {
        client: Client,
        subscription_counter: usize
    }

    impl RedisManager {
        fn new(url: &str) -> Option<RedisManager> {
            match simple_redis::create(url) {
                Ok(client) => Some(RedisManager { client, subscription_counter: 0 }),
                Err(_) => None
            }
        }
    }

    impl TopicManager for RedisManager {
        fn publish(&mut self, topic_name: String, message: TopicMessage) {
            unimplemented!()
        }

        fn subscribe(&mut self, name: String) -> Option<SubscriptionHandle> {
            self.client.subscribe(name.as_str());

            self.subscription_counter += 1;

            let topic = MemTopic::new(name);
            Some(Rc::new(RefCell::new(TopicSubscription {
                topic,
                id: self.subscription_counter
            })))    
        }

        fn poll(&mut self, handle: SubscriptionHandle, message_handler: &dyn Fn(TopicMessage)) {
            message_handler(TopicMessage {
                body: &[0x41]
            })
        }

        fn unsubscribe(&mut self, handle: SubscriptionHandle) {
            let sub = (*handle).borrow();
            self.client.unsubscribe(&(*sub).topic().name());
        }
    }

    #[test]
    fn redis_tdd() -> () {
        let mut manager = RedisManager::new("redis://127.0.0.1:6379/").unwrap();

        let _sub = manager.subscribe("test".to_string()).unwrap();

        manager.poll(_sub.clone(), &|message| {
            println!("Message: {:?}", message.body)
        });

        manager.unsubscribe(_sub.clone());
      /*  let mut client = simple_redis::create("redis://127.0.0.1:6379/").unwrap();
        let mut _result = client.subscribe("test");

        client.fetch_messages(&mut |message: Message| -> bool {
            let payload : String = message.get_payload().unwrap();
            println!("{}: {}", message.get_channel_name(), payload);
            false
        }, &mut || -> Interrupts {
            Interrupts::new()
        });*/
        println!("out")
    }
}
