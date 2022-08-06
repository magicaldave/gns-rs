use gns::*;
use std::{
    collections::HashMap,
    net::Ipv6Addr,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

// **unwrap** must be banned in production. unless you **know** what you are doing.

fn server(port: u16) {
    // Initialize valve GameNetworkingSocket library, this handle is unique, creating 2 instance would fail.
    // Dropping/recreating the instance is allowed though.
    // **unwrap** must be banned in production.
    let gns_global = GnsGlobal::get().unwrap();

    // **unwrap** must be banned in production.
    let gns_utils = GnsUtils::new().unwrap();

    // Minimalistic server state.
    // Map from client -> nickname.
    // The nickname is autogenerated (nonce incremented for each new connection).
    let mut connected_clients = HashMap::<GnsConnection, String>::new();
    let mut nonce = 0;

    // Initialize the server.
    // Note that GnsSocket implement drop for both server/client.
    // For the server, the listen socket + poll group are closed/cleaned up.
    // For the client, the connection is closed.
    let server = GnsSocket::new(&gns_global, &gns_utils)
        // **unwrap** must be banned in production.
        .unwrap()
        .listen(Ipv6Addr::LOCALHOST, port)
        // **unwrap** must be banned in production.
        .unwrap();

    // Setup debugging to log everything.
    // The current rust implementation flush the log in stdout.
    server.utils().enable_debug_output(
        ESteamNetworkingSocketsDebugOutputType::k_ESteamNetworkingSocketsDebugOutputType_Everything,
    );

    let mut last_update = Instant::now();
    loop {
        // Every 10 seconds, for each clients, print some stats:
        // IP, Ping, Outgoing bytes per sec, Incoming bytes per sec...
        let now = Instant::now();
        let elapsed = now - last_update;
        if elapsed.as_secs() > 10 {
            last_update = now;
            for (client, nick) in connected_clients.clone().into_iter() {
                // **unwrap** must be banned in production.
                let info = server.get_connection_info(client).unwrap();
                // **unwrap** must be banned in production.
                let (status, _) = server.get_connection_real_time_status(client, 0).unwrap();
                println!(
                  "== Client {:#?}\n\tIP: {:#?}\n\tPing: {:#?}\n\tOut/sec: {:#?}\n\tIn/sec: {:#?}",
                    nick,
                    info.remote_address(),
                    status.ping(),
                    status.out_bytes_per_sec(),
                    status.in_bytes_per_sec(),
                );
            }
        }

        // Poll internal callbacks
        server.poll_callbacks();

        // Broadcast a message to the provided clients.
        // We first build a list of messages and then send them.
        let broadcast_chat = |clients: Vec<GnsConnection>, title: &str, content: &str| {
            let messages = clients
                .clone()
                .into_iter()
                .map(|client| {
                    server.utils().allocate_message(
                        client,
                        k_nSteamNetworkingSend_Reliable,
                        format!("[{}]: {}", title, content).as_bytes(),
                    )
                })
                .collect::<Vec<_>>();
            // Here we should check whether all messages were successfully sent with the result.
            server.send_messages(messages);
        };

        // Process connections events.
        let _events_processed = server.poll_event::<100, _>(|event| {
          match (event.old_state(), event.info().state()) {
            // A client is about to connect, accept it.
            (
              ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_None,
              ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_Connecting,
            ) => {
              let result = server.accept(event.connection());
              println!("GnsSocket<Server>: accepted new client: {:#?}.", result);
              if result.is_ok() {
                connected_clients.insert(event.connection(), nonce.to_string());
                broadcast_chat(
                  connected_clients.keys().copied().collect(),
                  "Server",
                  &format!("A new user joined us, weclome {}", nonce),
                );
                nonce += 1;
              }
              println!("GnsSocket<Server>: number of clients: {:#?}.", connected_clients.len());
            }

            // A client is connected, we previously accepted it and don't do anything here.
            // In a more sophisticated scenario we could initial sending some messages.
            (
              ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_Connecting,
              ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_Connected,
            ) => {
            }

            (_, ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_ClosedByPeer | ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_ProblemDetectedLocally) => {
              // Remove the client from the list and close the connection.
              let conn = event.connection();
              println!("GnsSocket<Server>: {:#?} disconnected", conn);
              let nickname = &connected_clients[&conn];
              broadcast_chat(
                connected_clients.keys().copied().collect(),
                "Server",
                &format!("[{}] lost faith.", nickname),
              );
              connected_clients.remove(&conn);
              // Make sure we cleanup the connection, mandatory as per GNS doc.
              server.close_connection(conn, 0, "", false);
            }

            // A client state is changing, perhaps disconnecting
            // If a client disconnected and it's connection get cleaned up, its state goes back to `ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_None`
            (previous, current) => {
              println!("GnsSocket<Server>: {:#?} => {:#?}.", previous, current);
            }
          }
        });

        // Process some messages, we arbitrary define 100 as being the max number of messages we can handle per iteration.
        let _messages_processed = server.poll_messages::<100, _>(|message| {
            // **unwrap** must be banned in production.
            let chat_message = core::str::from_utf8(message.payload()).unwrap();
            println!("Boarcasting {}", chat_message);
            let sender = message.connection();
            let sender_nickname = &connected_clients[&sender];
            broadcast_chat(
                connected_clients.keys().copied().collect(),
                &sender_nickname,
                chat_message,
            );
        });

        std::thread::sleep(Duration::from_millis(10))
    }
}

fn user_input() -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || loop {
        let mut line = String::new();
        // **unwrap** must be banned in production.
        std::io::stdin().read_line(&mut line).unwrap();
        // **unwrap** must be banned in production.
        tx.send(line).unwrap();
    });
    rx
}

// Everything is pretty similar to the server.
fn client(port: u16) {
    // **unwrap** must be banned in production.
    let gns_global = GnsGlobal::get().unwrap();

    // **unwrap** must be banned in production.
    let gns_utils = GnsUtils::new().unwrap();

    let client = GnsSocket::new(&gns_global, &gns_utils)
        // **unwrap** must be banned in production.
        .unwrap()
        .connect(Ipv6Addr::LOCALHOST, port)
        // **unwrap** must be banned in production.
        .unwrap();

    client.utils().enable_debug_output(
        ESteamNetworkingSocketsDebugOutputType::k_ESteamNetworkingSocketsDebugOutputType_Everything,
    );

    let user_input_stream = user_input();

    'a: loop {
        client.poll_callbacks();

        // Process some messages, we arbitrary define 100 as being the max number of messages we can handle per iteration.
        let _messages_processed = client.poll_messages::<100, _>(|message| {
            println!(
                "(Chat) {}",
                // **unwrap** must be banned in production.
                core::str::from_utf8(message.payload()).unwrap()
            );
        });

        let mut quit = false;
        let _ =
            client.poll_event::<100, _>(|event| match (event.old_state(), event.info().state()) {
                (
                    ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_None,
                    ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_Connecting,
                ) => {
                    println!("GnsSocket<Client>: connecting to server.");
                }
                (
                    ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_Connecting,
                    ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_Connected,
                ) => {
                    println!("GnsSocket<Client>: connected to server.");
                }
                (_, ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_ClosedByPeer | ESteamNetworkingConnectionState::k_ESteamNetworkingConnectionState_ProblemDetectedLocally) => {
                  // We got disconnected or lost the connection.
                  println!("GnsSocket<Client>: ET phone home.");
                  quit = true;
                }
                (previous, current) => {
                    println!("GnsSocket<Client>: {:#?} => {:#?}.", previous, current);
                }

            });
        if quit {
            break 'a;
        }

        for input in user_input_stream.try_iter() {
            let input = input.trim();
            if input == "quit" {
                break 'a;
            }
            client.send_messages(vec![client.utils().allocate_message(
                client.connection(),
                k_nSteamNetworkingSend_Reliable,
                input.as_bytes(),
            )]);
        }

        std::thread::sleep(Duration::from_millis(10))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port = 55000;
    match args.get(1).expect("server or client expected").as_str() {
        "server" => {
            server(port);
        }
        "client" => {
            client(port);
        }
        _ => panic!("either client or server"),
    }
}
