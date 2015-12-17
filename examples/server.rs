extern crate smtpd;

use smtpd::SmtpServer;

fn main() {
    let mut smtp = SmtpServer::new();
    
    smtp.start_listener_thread("0.0.0.0:25").unwrap();

    for mail in smtp {
        println!("----- Receiving mail -----");
        println!("Mail from: {}", mail.from);
        println!("Mail to: {}", mail.rcpt);
        println!("{}", mail.message_body);
        println!("----- End of mail -----");
        println!("");
    }
}
