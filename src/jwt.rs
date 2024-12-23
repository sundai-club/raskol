use jsonwebtoken::Algorithm;

pub type Result<T> = jsonwebtoken::errors::Result<T>;

#[derive(Default, Debug, Clone)]
pub struct Options {
    pub secret: String,
    pub audience: String,
    pub issuer: String,
}

pub fn encode<T>(claims: &T, opt: &Options) -> Result<String>
where
    T: serde::Serialize,
{
    let str = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        claims,
        &jsonwebtoken::EncodingKey::from_secret(opt.secret.as_bytes()),
    )?;
    Ok(str)
}

pub fn decode<T>(str: &str, opt: &Options) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let mut validation_opts = jsonwebtoken::Validation::new(Algorithm::HS256);
    validation_opts.leeway = 0; // "exp" should mean what it says.
    validation_opts.set_audience(&[&opt.audience]);
    validation_opts.set_issuer(&[&opt.issuer]);
    let key = jsonwebtoken::DecodingKey::from_secret(opt.secret.as_bytes());
    let jsonwebtoken::TokenData { claims, .. } =
        jsonwebtoken::decode::<T>(str, &key, &validation_opts)?;
    Ok(claims)
}
