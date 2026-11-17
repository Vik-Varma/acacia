use acacia_auth::ClientData;
use acacia_auth::login::Skin;
use acacia_session::proto::GAME_VERSION;
use p384::ecdsa::SigningKey;

use crate::client::{ClientDataFn, Login};

/// Who the player is on the server: gamertag (or offline name) and xuid (empty offline).
pub(crate) struct Identity {
    pub display_name: String,
    pub xuid: String,
}

/// `nonce` is a friend's world's per-player join nonce (ClientData `Nonce`). `offline_key` signs an
/// offline login; an online one brings its own key. `adjust` edits the default client data, then
/// `mouse_input` and `skin` are applied.
pub(crate) fn build_login(
    login: Login,
    offline_key: SigningKey,
    server: &str,
    nonce: Option<String>,
    skin: Option<Skin>,
    mouse_input: bool,
    adjust: Option<ClientDataFn>,
) -> (SigningKey, Vec<u8>, Identity) {
    match login {
        Login::Offline { name } => {
            let key = offline_key;
            let client = client_data(&name, server, nonce, skin, mouse_input, adjust);
            let request = acacia_auth::build_offline_connection_request(&name, &key, &client);
            (
                key,
                request,
                Identity {
                    display_name: name,
                    xuid: String::new(),
                },
            )
        }
        Login::Online { credentials, key } => {
            let client = client_data(
                &credentials.display_name,
                server,
                nonce,
                skin,
                mouse_input,
                adjust,
            );
            let request = acacia_auth::build_connection_request(&credentials, &key, &client);
            let identity = Identity {
                display_name: credentials.display_name,
                xuid: credentials.xuid,
            };
            (key, request, identity)
        }
    }
}

/// The client data a login as `display_name` signs: the defaults, then `adjust`, then the nonce,
/// `mouse_input` and `skin`.
pub(crate) fn client_data(
    display_name: &str,
    server: &str,
    nonce: Option<String>,
    skin: Option<Skin>,
    mouse_input: bool,
    adjust: Option<ClientDataFn>,
) -> ClientData {
    let mut client = ClientData::default_for(display_name, server, GAME_VERSION);
    if let Some(adjust) = adjust {
        adjust(&mut client);
    }
    client.nonce = nonce;
    if mouse_input {
        client.current_input_mode = acacia_auth::login::INPUT_MODE_MOUSE;
    }
    if let Some(skin) = skin {
        client.skin = skin;
    }
    client
}

#[cfg(test)]
mod tests {
    use acacia_auth::ClientData;
    use acacia_auth::login::{DEVICE_OS_ANDROID, INPUT_MODE_TOUCH};
    use acacia_auth::login::{SigningKeys, Verifier};
    use p384::ecdsa::SigningKey;

    use super::build_login;
    use crate::client::{ClientDataFn, Login};

    fn signed_client_data(adjust: Option<ClientDataFn>, nonce: Option<String>) -> ClientData {
        let key = SigningKey::random(&mut rand_core::OsRng);
        let login = Login::Offline {
            name: "Desk".into(),
        };
        let (_, request, _) =
            build_login(login, key, "127.0.0.1:19132", nonce, None, false, adjust);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Verifier::new(SigningKeys::empty())
            .verify(&request, now)
            .unwrap()
            .client_data
    }

    #[test]
    fn client_data_defaults_to_an_android_phone() {
        let client = signed_client_data(None, None);
        assert_eq!(
            (
                client.device_os,
                client.current_input_mode,
                client.default_input_mode
            ),
            (DEVICE_OS_ANDROID, INPUT_MODE_TOUCH, INPUT_MODE_TOUCH)
        );
    }

    #[test]
    fn adjusted_client_data_is_signed_into_the_login() {
        // Win32 and Mouse, as a desktop client would report them.
        let adjust: ClientDataFn = Box::new(|client| {
            client.device_os = 8;
            client.current_input_mode = 1;
            client.default_input_mode = 1;
            client.nonce = Some("overwritten".into());
        });
        let client = signed_client_data(Some(adjust), Some("friend-nonce".into()));
        assert_eq!(
            (
                client.device_os,
                client.current_input_mode,
                client.default_input_mode
            ),
            (8, 1, 1)
        );
        assert_eq!(
            client.nonce.as_deref(),
            Some("friend-nonce"),
            "a friend's world's nonce wins over the adjustment"
        );
        assert_eq!(
            client.third_party_name, "Desk",
            "untouched claims keep their defaults"
        );
    }
}
