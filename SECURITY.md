# Security policy

## Supported versions

Acacia is pre-release. Only the latest commit on `master` gets fixes.

## Reporting a vulnerability

Please don't open a public issue. Report it privately through GitHub's
[**Report a vulnerability**](https://github.com/acacia-rs/acacia/security/advisories/new) form, with
the commit you tested, what the other side has to send, and what happens.

You'll get a reply within a week. Once a fix is on `master`, the advisory is published with credit to
you unless you'd rather stay anonymous.

## Scope

Acacia talks to servers, proxies and Microsoft's sign-in services it doesn't control, so these count:

- anything a server or peer can send that crashes a client, hangs it or makes memory grow without bound
- leaking sign-in tokens or keys: in logs, to a server, through a proxy, or from the token cache
- accepting a forged or tampered login chain or encryption handshake
- `acacia-mitm` exposing a recorded session's credentials beyond the files it writes

Gameplay bugs and a bot being detectable as a bot are ordinary issues.
