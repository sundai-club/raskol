use std::time::{Duration, SystemTime, SystemTimeError, UNIX_EPOCH};

use crate::conf;

use super::jwt;

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct Claims {
    pub sub: String,
    pub exp: u64,
    pub role: String,
    pub iss: String,
    pub aud: String,
    pub iat: u64,
    pub nbf: Option<u64>,
}

impl Claims {
    pub fn new(sub: &str, ttl: Duration) -> Result<Self, SystemTimeError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        let exp = now.saturating_add(ttl).as_secs();
        let sub = sub.to_string();
        let role = "HACKER".to_string();
        Ok(Self { sub, exp, role, iss: "".to_string(), aud: "".to_string(), iat: 0, nbf: None })
    }

    pub fn to_str(&self, jwt_conf: &conf::Jwt) -> jwt::Result<String> {
        jwt::encode(self, jwt_conf)
    }

    pub fn from_str(str: &str, jwt_conf: &conf::Jwt) -> jwt::Result<Self> {
        let claims = jwt::decode::<Self>(str, jwt_conf)?;
        
        if claims.iss != jwt_conf.issuer || claims.aud != jwt_conf.audience {
            tracing::warn!(
                expected_issuer = ?jwt_conf.issuer,
                actual_issuer = ?claims.iss,
                expected_audience = ?jwt_conf.audience,
                actual_audience = ?claims.aud,
                "JWT issuer or audience mismatch"
            );
            return Err(jsonwebtoken::errors::Error::from(
                jsonwebtoken::errors::ErrorKind::InvalidIssuer
            ));
        }
        
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jsonwebtoken::errors::ErrorKind;

    use crate::conf;

    use super::Claims;

    #[test]
    fn good() {
        let claims = Claims::new("foo", Duration::from_secs(5)).unwrap();
        let conf = conf::Jwt::default();
        let encoded: String = claims.to_str(&conf).unwrap();
        let decoded = Claims::from_str(&encoded, &conf).unwrap();
        assert_eq!(&claims, &decoded);
    }

    #[test]
    fn bad_key() {
        let claims = Claims::new("foo", Duration::from_secs(5)).unwrap();

        let conf_good = conf::Jwt::default();
        let conf_bad = conf::Jwt {
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
        let conf = conf::Jwt {
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
