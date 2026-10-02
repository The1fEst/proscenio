use aes::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
use crypto_bigint::modular::{FixedMontyForm, FixedMontyParams};
use crypto_bigint::{Odd, U1536};
use gtk4::glib;
use hkdf::Hkdf;
use sha2::Sha256;

const GROUP: &str = "sx-aes-1";
const PRIME: U1536 = U1536::from_be_hex(concat!(
    "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD1",
    "29024E088A67CC74020BBEA63B139B22514A08798E3404DD",
    "EF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245",
    "E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7ED",
    "EE386BFB5A899FA5AE9F24117C4B1FE649286651ECE45B3D",
    "C2007CB8A163BF0598DA48361C55D39A69163FA8FD24CF5F",
    "83655D23DCA3AD961C62F356208552BB9ED529077096966D",
    "670C354E4ABC9804F1746C08CA237327FFFFFFFFFFFFFFFF",
));
const PRIME_BYTES: usize = 192;
const KEY_BYTES: usize = 16;
const IV_BYTES: usize = 16;

type Encryptor = cbc::Encryptor<aes::Aes128>;
type Decryptor = cbc::Decryptor<aes::Aes128>;

pub struct SecretExchange {
    private: U1536,
    public: Vec<u8>,
    key: Option<[u8; KEY_BYTES]>,
}

impl SecretExchange {
    pub fn new() -> Option<Self> {
        let mut random = [0u8; PRIME_BYTES];
        getrandom::fill(&mut random).ok()?;
        random[0] = 0;
        let private = U1536::from_be_slice(&random);
        random.fill(0);
        let public = unpadded(&power(&U1536::from(2u32), &private)?);
        Some(SecretExchange {
            private,
            public,
            key: None,
        })
    }

    pub fn begin(&self) -> String {
        self.encode(None)
    }

    pub fn receive(&mut self, exchange: &str) -> Result<Option<Vec<u8>>, ()> {
        let file = glib::KeyFile::new();
        file.load_from_data(exchange, glib::KeyFileFlags::NONE)
            .map_err(|_| ())?;
        let peer = decoded(&file, "public").ok_or(())?;
        if peer.is_empty() || peer.len() > PRIME_BYTES {
            return Err(());
        }
        let mut padded = [0u8; PRIME_BYTES];
        padded[PRIME_BYTES - peer.len()..].copy_from_slice(&peer);
        let peer = U1536::from_be_slice(&padded);
        if peer <= U1536::ONE || peer >= PRIME.wrapping_sub(&U1536::ONE) {
            return Err(());
        }
        let shared = power(&peer, &self.private).ok_or(())?;
        let mut key = [0u8; KEY_BYTES];
        Hkdf::<Sha256>::new(None, &shared)
            .expand(&[], &mut key)
            .map_err(|_| ())?;
        self.key = Some(key);
        let Some(secret) = decoded(&file, "secret") else {
            return Ok(None);
        };
        let iv = decoded(&file, "iv").ok_or(())?;
        let iv: [u8; IV_BYTES] = iv.try_into().map_err(|_| ())?;
        Decryptor::new(&key.into(), &iv.into())
            .decrypt_padded_vec::<Pkcs7>(&secret)
            .map(Some)
            .map_err(|_| ())
    }

    pub fn send(&self, secret: &[u8]) -> Option<String> {
        let key = self.key?;
        let mut iv = [0u8; IV_BYTES];
        getrandom::fill(&mut iv).ok()?;
        let sealed = Encryptor::new(&key.into(), &iv.into()).encrypt_padded_vec::<Pkcs7>(secret);
        Some(self.encode(Some((&sealed, &iv))))
    }

    fn encode(&self, secret: Option<(&[u8], &[u8; IV_BYTES])>) -> String {
        let file = glib::KeyFile::new();
        file.set_string(GROUP, "public", &glib::base64_encode(&self.public));
        if let Some((sealed, iv)) = secret {
            file.set_string(GROUP, "secret", &glib::base64_encode(sealed));
            file.set_string(GROUP, "iv", &glib::base64_encode(iv));
        }
        file.to_data().to_string()
    }
}

fn power(base: &U1536, exponent: &U1536) -> Option<[u8; PRIME_BYTES]> {
    let modulus = Option::<Odd<U1536>>::from(Odd::new(PRIME))?;
    let params = FixedMontyParams::new_vartime(modulus);
    let result = FixedMontyForm::new(base, &params).pow(exponent).retrieve();
    result.to_be_bytes().as_ref().try_into().ok()
}

fn unpadded(bytes: &[u8]) -> Vec<u8> {
    let start = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    bytes[start..].to_vec()
}

fn decoded(file: &glib::KeyFile, key: &str) -> Option<Vec<u8>> {
    file.string(GROUP, key)
        .ok()
        .map(|text| glib::base64_decode(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_crosses_between_two_exchanges() {
        let mut caller = SecretExchange::new().unwrap();
        let mut prompter = SecretExchange::new().unwrap();
        assert_eq!(prompter.receive(&caller.begin()), Ok(None));
        let sent = prompter.send(b"correct horse").unwrap();
        assert_eq!(caller.receive(&sent), Ok(Some(b"correct horse".to_vec())));
    }

    #[test]
    fn the_exchange_is_a_key_file_with_an_unpadded_public_key() {
        let exchange = SecretExchange::new().unwrap();
        let text = exchange.begin();
        assert!(text.starts_with("[sx-aes-1]\npublic="), "{text}");
        assert_ne!(exchange.public.first(), Some(&0));
        assert!(!text.contains("secret="));
    }

    #[test]
    fn nothing_is_sent_before_the_peer_key_arrives() {
        assert!(SecretExchange::new().unwrap().send(b"x").is_none());
    }

    #[test]
    fn a_degenerate_peer_key_is_refused() {
        let mut exchange = SecretExchange::new().unwrap();
        let one = format!("[{GROUP}]\npublic={}\n", glib::base64_encode(&[1]));
        assert_eq!(exchange.receive(&one), Err(()));
    }
}
