use std::time::{Duration, SystemTime, SystemTimeError, UNIX_EPOCH};

use crate::conf::ConfJwt;

use super::jwt;

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
pub struct Claims {
    pub sub: String,
    exp: u64,
}

impl Claims {
    pub fn new(sub: &str, ttl: Duration) -> Result<Self, SystemTimeError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        let exp = now.saturating_add(ttl).as_secs();
        let sub = sub.to_string();
        Ok(Self { sub, exp })
    }

    pub fn to_str(&self, jwt_conf: &ConfJwt) -> jwt::Result<String> {
        jwt::encode(self, jwt_conf)
    }

    pub fn from_str(str: &str, jwt_conf: &ConfJwt) -> jwt::Result<Self> {
        jwt::decode::<Self>(str, jwt_conf)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jsonwebtoken::errors::ErrorKind;

    use crate::conf::ConfJwt;

    use super::Claims;

    #[test]
    fn good() {
        let claims = Claims::new("foo", Duration::from_secs(5)).unwrap();
        let conf = ConfJwt::default();
        let encoded: String = claims.to_str(&conf).unwrap();
        let decoded = Claims::from_str(&encoded, &conf).unwrap();
        assert_eq!(&claims, &decoded);
    }

    #[test]
    fn bad_key() {
        let claims = Claims::new("foo", Duration::from_secs(5)).unwrap();

        let conf_good = ConfJwt::default();
        let conf_bad = ConfJwt {
            secret: conf_good.secret.to_string() + "naughty",
            ..conf_good.clone()
        };

        let encoded: String = claims.to_str(&conf_good).unwrap();
        let decode_result = Claims::from_str(&encoded, &conf_bad);

        assert!(matches!(
            decode_result,
            Err(e) if e.kind().eq(&ErrorKind::InvalidSignature)
        ));
    }

    #[test]
    fn expired() {
        let conf = ConfJwt {
            secret: "super secret".to_string(),
            ..Default::default()
        };

        let mut claims = Claims::new("foo", Duration::ZERO).unwrap();
        claims.exp -= 10; // Expire arbitrarily-far back in the past.

        let encoded: String = claims.to_str(&conf).unwrap();
        let decode_result = Claims::from_str(&encoded, &conf);
        dbg!(&decode_result);

        assert!(matches!(
            decode_result,
            Err(e) if e.kind().eq(&ErrorKind::ExpiredSignature)
        ));
    }
}
