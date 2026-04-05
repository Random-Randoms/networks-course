use lettre::{
    Message, SmtpTransport, Transport,
    message::{Mailbox, header::ContentType},
    transport::smtp::authentication::Credentials,
};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    sender_addr: String,

    #[arg(long, default_value=None)]
    sender_name: Option<String>,

    #[arg(long)]
    receiver_addr: String,

    #[arg(long, default_value=None)]
    receiver_name: Option<String>,

    #[arg(short, long, default_value="")]
    subject: String,

    #[arg(short, long)]
    message: String,

    #[arg(short, long)]
    creds: String,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    let content_type = match std::path::Path::new(args.message.as_str()).extension() {
        Some(ext) => if ext.eq("html") { ContentType::TEXT_HTML } else { ContentType::TEXT_PLAIN },
        _ => ContentType::TEXT_PLAIN,
    };
    let text = std::fs::read(args.message)?;
    let creds_text = std::fs::read_to_string(args.creds)?;
    let (smtp_uname, smtp_pword) = creds_text
        .split_once(" ")
        .expect("creds file should contain only smpt username and password, separated by space");

    let email = Message::builder()
        .from(Mailbox::new(args.sender_name, args.sender_addr.parse()?))
        .to(Mailbox::new(args.receiver_name, args.receiver_addr.parse()?))
        .subject(args.subject)
        .header(content_type)
        .body(text)?;

    let creds = Credentials::new(smtp_uname.to_owned(), smtp_pword.to_owned());

    let mailer = SmtpTransport::relay("smpt.mail.ru")
        ?.credentials(creds)
        .build();

    match mailer.send(&email) {
        Ok(_) => println!("Email sent successfully!"),
        Err(e) => panic!("Could not send email: {e:?}"),
    }

    Ok(())
}
