use std::{
    fs,
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    thread::sleep,
    time::Duration,
};

use assert_cmd::{assert::OutputAssertExt, cargo::CommandCargoExt};

#[tokio::test]
async fn ping() {
    let exe = env!("CARGO_PKG_NAME");
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();

    let raskol::conf::Conf {
        addr, port, tls, ..
    } = setup_conf(&dir);
    let tls = tls.unwrap();
    let cert = fs::read(&tls.cert_file).unwrap();
    let cert = reqwest::Certificate::from_pem(cert.trim_ascii()).unwrap();
    let client = reqwest::Client::builder()
        .add_root_certificate(cert)
        .build()
        .unwrap();
    let cmd = || {
        let mut cmd = Command::cargo_bin(exe).unwrap();
        cmd.arg("--dir").arg(dir);
        cmd
    };

    let sock_addr: SocketAddr = format!("{addr}:{port}").parse().unwrap();
    assert!(server_is_not_listening(&sock_addr));
    let mut server = cmd().arg("server").spawn().unwrap();
    assert!(server_is_listening(&sock_addr));

    let resp = client
        .get(format!("https://{addr}:{port}/ping"))
        .send()
        .await;

    // XXX Stop the server BEFORE asserting, because if any assert fails
    //     we will not get a chance to clean-up.
    server.kill().unwrap();

    let resp = resp.unwrap();
    let status = resp.status();
    assert!(status.is_success());
}

fn setup_conf(workdir: &Path) -> raskol::conf::Conf {
    let (cert_file, key_file) = setup_cert(workdir);
    let conf = raskol::conf::Conf {
        log_level: tracing::Level::INFO,
        addr: "127.0.0.1".parse().unwrap(),
        port: 7000,
        jwt: raskol::conf::ConfJwt {
            secret: "fake-secret".to_string(),
            audience: "fake-audience".to_string(),
            issuer: "fake-issuer".to_string(),
        },
        target_address: "127.0.0.1:7001".to_string(),
        target_auth_token: String::new(),
        min_hit_interval: 5.0,
        max_tokens_per_day: 10,
        sqlite_busy_timeout: 60.0,
        tls: Some(raskol::conf::Tls {
            cert_file: cert_file.clone(),
            key_file: key_file.clone(),
        }),
    };
    let conf_str = toml::to_string(&conf).unwrap();
    let conf_dir = workdir.join("conf");
    fs::create_dir_all(&conf_dir).unwrap();
    fs::write(conf_dir.join("conf.toml"), &conf_str).unwrap();
    conf
}

fn setup_cert(workdir: &Path) -> (PathBuf, PathBuf) {
    let cert_dir = workdir.join("cert");
    fs::create_dir_all(&cert_dir).unwrap();
    let cert_file = cert_dir.join("cert.pem");
    let key_file = cert_dir.join("key.pem");

    #[rustfmt::skip] // I want the args to stay paired.
    Command::new("openssl").args([
        "req", "-x509",
        "-newkey", "rsa:4096",
        "-days", "365",
        "-nodes",
        "-subj", "/CN=localhost",
        "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1",
    ])
    .arg("-keyout")
    .arg(&key_file)
    .arg("-out")
    .arg(&cert_file)
    .assert();
    (
        cert_file.canonicalize().unwrap(),
        key_file.canonicalize().unwrap(),
    )
}

fn server_is_not_listening(addr: &SocketAddr) -> bool {
    TcpStream::connect(addr).is_err()
}

fn server_is_listening(addr: &SocketAddr) -> bool {
    let interval = Duration::from_secs_f32(0.25);
    let attempts = 10;
    retry_until_true(|| TcpStream::connect(addr).is_ok(), interval, attempts)
}

fn retry_until_true<F: Fn() -> bool>(
    f: F,
    interval: Duration,
    mut attempts: usize,
) -> bool {
    while attempts > 0 {
        if f() {
            return true;
        } else {
            attempts -= 1;
            sleep(interval);
        }
    }
    false
}
