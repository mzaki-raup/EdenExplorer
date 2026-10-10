//! SCRAM-SHA-256 (RFC 7677), the challenge-response MongoDB signs in with.
//!
//! The password never crosses the wire: the client proves it knows it by
//! signing a transcript of the exchange. Three messages each way:
//!
//! ```text
//! client-first   n,,n=<user>,r=<client nonce>
//! server-first   r=<client nonce + server nonce>,s=<salt>,i=<iterations>
//! client-final   c=biws,r=<nonce>,p=<proof>
//! server-final   v=<server signature>
//! ```
//!
//! The server's own signature is checked too, which is the half that is easy
//! to skip and shouldn't be: without it a server that never knew the password
//! could still be believed.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub struct Scram {
    /// `n=user,r=nonce` - the first message without its GS2 header, which is
    /// what the transcript is built from.
    first_bare: String,
    nonce: String,
    password: String,
    /// Filled in by `client_final`, checked by `verify_server`.
    server_signature: Option<Vec<u8>>,
}

pub(super) fn hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

fn sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// PBKDF2-HMAC-SHA-256, which SCRAM uses to salt and stretch the password.
///
/// Written out rather than pulled in: it is the standard construction - U1 =
/// HMAC(password, salt || 0x00000001), then each further U is the HMAC of the
/// previous, all XORed together - and one output block is all SCRAM needs,
/// since SHA-256's output is exactly the key length.
pub(super) fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32) -> Vec<u8> {
    let mut salted = salt.to_vec();
    salted.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac(password, &salted);
    let mut out = u.clone();
    for _ in 1..iterations {
        u = hmac(password, &u);
        for (acc, byte) in out.iter_mut().zip(u.iter()) {
            *acc ^= byte;
        }
    }
    out
}

/// A user name as SCRAM wants it: `=` and `,` are the field separators, so
/// they are escaped rather than allowed to break the message apart.
fn escape(user: &str) -> String {
    user.replace('=', "=3D").replace(',', "=2C")
}

/// One field of a `key=value,key=value` message.
fn field(message: &str, key: char) -> Option<&str> {
    message
        .split(',')
        .find_map(|part| part.strip_prefix(key)?.strip_prefix('='))
}

impl Scram {
    /// `nonce` must be unguessable - see `random_nonce`.
    pub fn new(user: &str, password: &str, nonce: String) -> Self {
        Scram {
            first_bare: format!("n={},r={nonce}", escape(user)),
            nonce,
            password: password.to_string(),
            server_signature: None,
        }
    }

    /// The first message, including the `n,,` GS2 header saying there is no
    /// channel binding.
    pub fn client_first(&self) -> Vec<u8> {
        format!("n,,{}", self.first_bare).into_bytes()
    }

    /// Answers the server's challenge, and remembers the signature the
    /// server will have to produce in return.
    pub fn client_final(&mut self, server_first: &[u8]) -> Result<Vec<u8>, String> {
        let server_first = String::from_utf8_lossy(server_first).into_owned();
        let nonce = field(&server_first, 'r').ok_or("the server sent no nonce")?;
        let salt = field(&server_first, 's').ok_or("the server sent no salt")?;
        let iterations: u32 = field(&server_first, 'i')
            .ok_or("the server sent no iteration count")?
            .parse()
            .map_err(|_| "the server sent a malformed iteration count")?;

        // The server must build on the nonce we sent, or it is replaying
        // somebody else's exchange at us.
        if !nonce.starts_with(&self.nonce) {
            return Err("the server's nonce doesn't extend ours".into());
        }
        if iterations == 0 {
            return Err("the server asked for no iterations".into());
        }

        let salt = super::unbase64(salt).ok_or("the server's salt isn't valid base64")?;
        let salted = pbkdf2(self.password.as_bytes(), &salt, iterations);

        let client_key = hmac(&salted, b"Client Key");
        let stored_key = sha256(&client_key);
        let without_proof = format!("c=biws,r={nonce}");
        let auth_message = format!("{},{server_first},{without_proof}", self.first_bare);

        let client_signature = hmac(&stored_key, auth_message.as_bytes());
        let proof: Vec<u8> = client_key
            .iter()
            .zip(client_signature.iter())
            .map(|(key, signature)| key ^ signature)
            .collect();

        let server_key = hmac(&salted, b"Server Key");
        self.server_signature = Some(hmac(&server_key, auth_message.as_bytes()));

        Ok(format!("{without_proof},p={}", super::base64(&proof)).into_bytes())
    }

    /// Checks the server proved it knew the password too.
    pub fn verify_server(&self, server_final: &[u8]) -> Result<(), String> {
        let text = String::from_utf8_lossy(server_final).into_owned();
        if let Some(error) = field(&text, 'e') {
            return Err(format!("the server refused the sign-in ({error})"));
        }
        let signature = field(&text, 'v').ok_or("the server sent no signature")?;
        let expected = self
            .server_signature
            .as_ref()
            .ok_or("nothing to check the server's signature against yet")?;
        let actual = super::unbase64(signature).ok_or("the server's signature isn't base64")?;
        if &actual != expected {
            return Err("the server's signature is wrong".into());
        }
        Ok(())
    }
}

/// A fresh client nonce. SCRAM's replay protection rests entirely on this
/// being unpredictable, so it comes from the system's own generator.
pub fn random_nonce() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..24)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7677's own worked example, which is the only way to be sure every
    /// step - the PBKDF2 block, the key derivation, the transcript, the XOR -
    /// is right rather than merely self-consistent.
    const USER: &str = "user";
    const PASSWORD: &str = "pencil";
    const CLIENT_NONCE: &str = "rOprNGfwEbeRWgbNEkqO";
    const SERVER_FIRST: &str =
        "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
    const CLIENT_FINAL: &str = "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,\
                                p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=";
    const SERVER_FINAL: &str = "v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=";

    #[test]
    fn the_rfc_7677_exchange_produces_the_rfc_7677_proof() {
        let mut scram = Scram::new(USER, PASSWORD, CLIENT_NONCE.to_string());
        assert_eq!(
            String::from_utf8(scram.client_first()).unwrap(),
            "n,,n=user,r=rOprNGfwEbeRWgbNEkqO"
        );

        let final_message = scram.client_final(SERVER_FIRST.as_bytes()).expect("final");
        assert_eq!(String::from_utf8(final_message).unwrap(), CLIENT_FINAL);

        scram
            .verify_server(SERVER_FINAL.as_bytes())
            .expect("the server's signature should check out");
    }

    #[test]
    fn a_server_that_cannot_sign_is_rejected() {
        let mut scram = Scram::new(USER, PASSWORD, CLIENT_NONCE.to_string());
        scram.client_final(SERVER_FIRST.as_bytes()).expect("final");

        // One byte different: a server that didn't actually know the
        // password could otherwise pass for one that did.
        let forged = "v=7rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=";
        assert!(scram.verify_server(forged.as_bytes()).is_err());
        assert!(scram.verify_server(b"v=not-base64!!").is_err());
        assert!(scram.verify_server(b"").is_err());
    }

    #[test]
    fn a_server_nonce_that_does_not_extend_ours_is_refused() {
        // The whole point of the client nonce is that the server must build
        // on it; one that doesn't is replaying another exchange.
        let mut scram = Scram::new(USER, PASSWORD, CLIENT_NONCE.to_string());
        let replayed = "r=someoneElsesNonce,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
        assert!(scram.client_final(replayed.as_bytes()).is_err());
    }

    #[test]
    fn a_malformed_challenge_is_an_error_rather_than_a_panic() {
        for challenge in [
            "",
            "r=rOprNGfwEbeRWgbNEkqO",                                  // no salt
            "r=rOprNGfwEbeRWgbNEkqO,s=W22ZaJ0SNY7soEsUEjb6gQ==",       // no count
            "r=rOprNGfwEbeRWgbNEkqO,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=0",   // zero count
            "r=rOprNGfwEbeRWgbNEkqO,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=abc", // not a number
            "r=rOprNGfwEbeRWgbNEkqO,s=not base64,i=4096",
        ] {
            let mut scram = Scram::new(USER, PASSWORD, CLIENT_NONCE.to_string());
            assert!(
                scram.client_final(challenge.as_bytes()).is_err(),
                "should have been refused: {challenge:?}"
            );
        }
    }

    #[test]
    fn a_user_name_containing_a_separator_is_escaped() {
        // `=` and `,` separate SCRAM's own fields; unescaped they would let a
        // user name forge extra ones.
        assert_eq!(escape("a,b=c"), "a=2Cb=3Dc");
        let scram = Scram::new("a,b", "x", "nonce".into());
        assert_eq!(
            String::from_utf8(scram.client_first()).unwrap(),
            "n,,n=a=2Cb,r=nonce"
        );
    }

    #[test]
    fn two_nonces_are_not_the_same() {
        let a = random_nonce();
        let b = random_nonce();
        assert_ne!(a, b);
        assert_eq!(a.len(), 24);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
