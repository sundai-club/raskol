use std::time::{Duration, SystemTime, SystemTimeError, UNIX_EPOCH};

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

    pub fn to_str(&self, jwt_opts: &jwt::Options) -> jwt::Result<String> {
        jwt::encode(self, jwt_opts)
    }

    pub fn from_str(str: &str, jwt_opts: &jwt::Options) -> jwt::Result<Self> {
        jwt::decode::<Self>(str, jwt_opts)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jsonwebtoken::errors::ErrorKind;

    use crate::jwt;

    use super::Claims;

    #[test]
    fn good() {
        let claims = Claims::new("foo", Duration::from_secs(5)).unwrap();

        let secret = "super-secret".to_string();
        let audience = "".to_string();
        let issuer = "".to_string();
        let opt = jwt::Options {
            secret,
            audience,
            issuer,
        };

        let encoded: String = claims.to_str(&opt).unwrap();
        let decoded = Claims::from_str(&encoded, &opt).unwrap();

        assert_eq!(&claims, &decoded);
    }

    #[test]
    fn bad_key() {
        let claims = Claims::new("foo", Duration::from_secs(5)).unwrap();

        let secret_good = "super secret";
        let secret_bad = secret_good.to_string() + " naughty";
        let opt_good = jwt::Options {
            secret: secret_good.to_string(),
            audience: "".to_string(),
            issuer: "".to_string(),
        };
        let opt_bad = jwt::Options {
            secret: secret_bad.to_string(),
            audience: "".to_string(),
            issuer: "".to_string(),
        };

        let encoded: String = claims.to_str(&opt_good).unwrap();
        let decode_result = Claims::from_str(&encoded, &opt_bad);

        assert!(matches!(
            decode_result,
            Err(e) if e.kind().eq(&ErrorKind::InvalidSignature)
        ));
    }

    #[test]
    fn expired() {
        let opt = jwt::Options {
            secret: "super secret".to_string(),
            ..Default::default()
        };

        let mut claims = Claims::new("foo", Duration::ZERO).unwrap();
        claims.exp -= 10; // Expire arbitrarily-far back in the past.

        let encoded: String = claims.to_str(&opt).unwrap();
        let decode_result = Claims::from_str(&encoded, &opt);
        dbg!(&decode_result);

        assert!(matches!(
            decode_result,
            Err(e) if e.kind().eq(&ErrorKind::ExpiredSignature)
        ));
    }
}
