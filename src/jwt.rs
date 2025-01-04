use jsonwebtoken::Algorithm;

use crate::conf;

pub type Result<T> = jsonwebtoken::errors::Result<T>;

pub fn encode<T>(claims: &T, conf: &conf::Jwt) -> Result<String>
where
    T: serde::Serialize,
{
    let str = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        claims,
        &jsonwebtoken::EncodingKey::from_secret(conf.secret.as_bytes()),
    )?;
    Ok(str)
}

pub fn decode<T>(str: &str, conf: &conf::Jwt) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let mut validation_opts = jsonwebtoken::Validation::new(Algorithm::HS256);
    validation_opts.leeway = 0; // "exp" should mean what it says.
    validation_opts.set_audience(&[&conf.audience]);
    validation_opts.set_issuer(&[&conf.issuer]);
    let key = jsonwebtoken::DecodingKey::from_secret(conf.secret.as_bytes());
    let jsonwebtoken::TokenData { claims, .. } =
        jsonwebtoken::decode::<T>(str, &key, &validation_opts)?;
    Ok(claims)
}
