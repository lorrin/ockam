use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use crate::config::{AddonKind, Config};
use attohttpc::post;
use ockam_channel::CHANNEL_ZERO;
use ockam_message::message::{
    Address, AddressType, Codec, Message as OckamMessage, Message, MessageType, Route,
    RouterAddress,
};
use ockam_system::commands::{ChannelCommand, OckamCommand, RouterCommand, WorkerCommand};
use std::io::Write;
use std::net::TcpStream;

type WorkFn = fn(self_worker: &SinkWorker, msg: OckamMessage);

#[allow(dead_code)]

pub struct SinkWorker {
    router_tx: Sender<OckamCommand>,
    channel_tx: Sender<OckamCommand>,
    rx: Receiver<OckamCommand>,
    tx: Sender<OckamCommand>,
    addr: RouterAddress,
    work_fn: WorkFn,
    config: Config,
    route: Option<Route>,
}

impl SinkWorker {
    pub fn initialize(
        config: &Config,
        worker_addr: RouterAddress,
        router_tx: Sender<OckamCommand>,
        channel_tx: Sender<OckamCommand>,
    ) -> Result<SinkWorker, String> {
        let worker = SinkWorker::new(
            worker_addr,
            router_tx,
            channel_tx.clone(),
            config.clone(),
            |w, msg| match w.config().addon() {
                Some(AddonKind::InfluxDb(url, db)) => {
                    let payload = String::from_utf8(msg.message_body);
                    if payload.is_err() {
                        eprintln!("invalid message body for influx");
                        return;
                    }

                    match post(format!("{}write?db={}", url.into_string(), db))
                        .text(payload.unwrap())
                        .send()
                    {
                        Ok(resp) => {
                            if let Err(e) = resp.error_for_status() {
                                eprintln!("bad influx HTTP response: {}", e);
                            }
                        }
                        Err(e) => println!("failed to send to influxdb: {}", e),
                    }
                },
                Some(AddonKind::Kafka(endpoint)) => {
                    let mut stream = TcpStream::connect(endpoint).expect("Can't establish connection to Kafka Connect Source");
                    stream.write(msg.message_body.as_ref());
                }
                None => {
                    let mut out = std::io::stdout();
                    out.write_all(msg.message_body.as_ref())
                        .expect("failed to write message to stdout");
                    out.flush().expect("failed to flush stdout");
                }
            },
        );
        Ok(worker)
    }

    pub fn new(
        addr: RouterAddress,
        router_tx: Sender<OckamCommand>,
        channel_tx: Sender<OckamCommand>,
        config: Config,
        work_fn: WorkFn,
    ) -> Self {
        debug_assert!(matches!(addr.a_type, AddressType::Worker));

        let (tx, rx) = mpsc::channel();

        // register the worker with the router
        let cmd = OckamCommand::Router(RouterCommand::Register(AddressType::Worker, tx.clone()));
        router_tx.send(cmd).expect("failed to register worker");

        println!("Service address: {}", addr.address.as_string());

        // kick off secure channel to router, if we have a router address
        match config.route_hub() {
            Some(socket) => {
                let route = Route {
                    addresses: vec![
                        RouterAddress::from_address(Address::TcpAddress(socket)).unwrap(),
                        RouterAddress::channel_router_address_from_str(CHANNEL_ZERO).unwrap(),
                    ],
                };
                channel_tx
                    .send(OckamCommand::Channel(ChannelCommand::Initiate(
                        route,
                        Address::WorkerAddress(hex::decode("01242020").unwrap()),
                        None,
                    )))
                    .unwrap();
            }
            None => {}
        }
        //------------------------------------

        SinkWorker {
            router_tx,
            channel_tx,
            rx,
            tx,
            addr,
            config,
            work_fn,
            route: None,
        }
    }

    pub fn sender(&self) -> Sender<OckamCommand> {
        self.tx.clone()
    }

    pub fn config(&self) -> Config {
        self.config.clone()
    }

    fn receive_channel(&mut self, m: Message) -> Result<(), String> {
        self.route = Some(m.return_route.clone());
        if !m.message_body.is_empty() {
            if let Ok((cleartext_channel_addr, _)) = RouterAddress::decode(&m.message_body) {
                println!(
                    "Channel cleartext address: {}",
                    cleartext_channel_addr.address.as_string()
                );
            }
        }
        Ok(())
    }

    pub fn poll(&mut self) -> bool {
        match self.rx.try_recv() {
            Ok(cmd) => match cmd {
                OckamCommand::Worker(WorkerCommand::ReceiveMessage(msg)) => {
                    match msg.message_type {
                        MessageType::Payload => {
                            // Confirm address
                            if self.addr != msg.onward_route.addresses[0] {
                                println!("Received bad worker address");
                                return true;
                            }
                            (self.work_fn)(&self, msg);
                            true
                        }
                        MessageType::None => {
                            return if let Err(s) = self.receive_channel(msg) {
                                println!("failed to receive channel: {}", s);
                                false
                            } else {
                                true
                            }
                        }
                        _ => unimplemented!(),
                    }
                }
                _ => {
                    eprintln!("unrecognized worker command: {:?}", cmd);
                    false
                }
            },
            Err(e) => match e {
                TryRecvError::Empty => true,
                _ => {
                    eprintln!("failed to recv worker rx: {:?}", e);
                    false
                }
            },
        }
    }
}

//#[test]
// fn test_ockamd_worker() {
//     let addr = RouterAddress::worker_router_address_from_str("01242020").unwrap();
//     let (fake_router_tx, fake_router_rx) = mpsc::channel();
//     Worker::new(addr, fake_router_tx, Default::default(), |_, _| {});
//
//     assert!(fake_router_rx.recv().is_ok());
// }
