//! Microsoft/Mojang account authentication.

pub struct Account {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
}

pub enum AuthError {
    DeviceCodeExpired,
    NetworkError,
    NotEntitled,
}

pub async fn login_microsoft() -> Result<Account, AuthError> {
    todo!("run the MS device-code OAuth flow, exchange for a Minecraft session")
}

pub async fn refresh(account: &Account) -> Result<Account, AuthError> {
    todo!("refresh an expired access token")
}
