use std::io::prelude::*;
use std::io;
use std::net::{TcpListener, TcpStream};
use std::io::BufReader;
use std::sync::mpsc;
use std::thread;

pub struct SmtpServer {
    send_channel: mpsc::Sender<SmtpMail>,
    recv_channel: mpsc::Receiver<SmtpMail>
}

impl SmtpServer {
    pub fn new() -> SmtpServer {
        let (send, recv) = mpsc::channel();
        return SmtpServer {
            send_channel: send,
            recv_channel: recv
        };
    }

    pub fn to_smtp_connection(&mut self, stream: TcpStream) -> SmtpConnection {
        return SmtpConnection {
            stream: stream,
            send_channel: self.send_channel.clone(),
            hostname: None,
            mailfrom: None,
            rcpt: None,
            message: String::new(),
            state: SmtpState::Command
        };
    }

    pub fn start_listener_thread(&mut self, addr: &str) -> io::Result<()> {
        let listener = try!(TcpListener::bind(addr));
        let send_channel = self.send_channel.clone();
        thread::spawn(move|| {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let mut conn = SmtpConnection::with_channel(stream, send_channel.clone());
                        thread::spawn(move|| {
                            let _ = conn.handle_connection();
                        });
                    },
                    Err(_) => {}
                }
            }
        });
        Ok(())
    }
}

impl Iterator for SmtpServer {
    type Item = SmtpMail;
    fn next(&mut self) -> Option<SmtpMail> {
        return self.recv_channel.recv().ok();
    }
}

pub struct SmtpConnection {
    stream: TcpStream,
    send_channel: mpsc::Sender<SmtpMail>,
    hostname: Option<String>,
    mailfrom: Option<String>,
    rcpt: Option<String>,
    message: String,
    state: SmtpState,
}

impl SmtpConnection {
    pub fn with_channel(stream: TcpStream, send: mpsc::Sender<SmtpMail>) -> SmtpConnection {
        return SmtpConnection {
            stream: stream,
            send_channel: send.clone(),
            hostname: None,
            mailfrom: None,
            rcpt: None,
            message: String::new(),
            state: SmtpState::Command
        };
    }

    pub fn handle_connection(&mut self) -> io::Result<()> {
        try!(self.stream.write_all(b"220 Rust smtpd v0.1.0\r\n"));
        let reader = BufReader::new(try!(self.stream.try_clone()));
        for line in reader.lines() {
            let line = try!(line);
            match self.line_received(&line) {
                Ok(_) => {
                    if self.state == SmtpState::Quit {
                        break;
                    }
                },
                Err(_) => {
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn line_received(&mut self, line: &str) -> std::io::Result<()> {
        match self.state {
            SmtpState::Command => {
                let space_pos = line.find(" ").unwrap_or(line.len());
                let (command, arg) = line.split_at(space_pos);
                let arg = arg.trim();

                match &*command.to_uppercase() {
                    "HELO" | "EHLO" => {
                        if !arg.is_empty() {
                            self.hostname = Some(arg.to_string());
                            try!(self.stream.write_all(format!("250 Hello {}\r\n", arg).as_bytes()));
                        }else {
                            try!(self.stream.write_all(b"501 Syntax: HELO hostname\r\n"));
                        }
                    },
                    "MAIL" => {
                        // Syntax MAIL From: <user@example.com>

                        let lower_arg = arg.to_lowercase();
                        if lower_arg.starts_with("from:") {
                            let angle_brackets: &[_] = &['<', '>'];
                            let address = lower_arg.trim_left_matches("from:")
                                .trim().trim_matches(angle_brackets).trim();

                            self.mailfrom = Some(address.to_string());
                            try!(self.stream.write_all(b"250 OK\r\n"));
                        }else {
                            try!(self.stream.write_all(b"501 Syntax: MAIL From: <address>\r\n"));
                        }
                    },
                    "RCPT" => {
                        // Syntax RCPT To: <user@example.com>

                        let lower_arg = arg.to_lowercase();
                        if lower_arg.starts_with("to:") {
                            let angle_brackets: &[_] = &['<', '>'];
                            let address = lower_arg.trim_left_matches("to:")
                                .trim().trim_matches(angle_brackets).trim();

                            self.rcpt = Some(address.to_string());
                            try!(self.stream.write_all(b"250 OK\r\n"));
                        }else {
                            try!(self.stream.write_all(b"501 Syntax: RCPT To: <address>\r\n"));
                        }
                    },
                    "DATA" => {
                        self.state = SmtpState::Data;
                        try!(self.stream.write_all(b"354 End data with <CRLF>.<CRLF>\r\n"));
                    },
                    "NOOP" => {
                        if arg.is_empty() {
                            try!(self.stream.write_all(b"250 OK\r\n"));
                        }else {
                            try!(self.stream.write_all(b"501 Syntax: NOOP\r\n"));
                        }
                    },
                    "RSET" => {
                        self.mailfrom = None;
                        self.rcpt = None;
                        self.message = String::new();

                        try!(self.stream.write_all(b"250 OK\r\n"));
                    },
                    "QUIT" => {
                        try!(self.stream.write_all(b"221 Have a nice day!\r\n"));
                        self.state = SmtpState::Quit;
                    },
                    x => {
                        try!(self.stream.write_all(format!("500 Error: Unknown command '{}'\r\n", x).as_bytes()));
                    }
                }
            },
            SmtpState::Data => {
                // Write message
                if line.trim() == "." {
                    try!(self.stream.write_all(b"250 OK\r\n"));
                    let mail = SmtpMail {
                        from: self.mailfrom.clone().unwrap(),
                        rcpt: self.rcpt.clone().unwrap(),
                        message_body: self.message.clone()

                    };
                    self.send_channel.send(mail).unwrap();
                }else {
                    self.message.push_str(line);
                    self.message.push_str("\n");
                }
            },
            SmtpState::Quit => {}
        }
        
        Ok(())
    }
}

#[derive(PartialEq)]
enum SmtpState {
    Command,
    Data,
    Quit
}

pub struct SmtpMail {
    pub from: String,
    pub rcpt: String,
    pub message_body: String
}
