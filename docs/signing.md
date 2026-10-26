# Signing SolidLauncher for macOS

Without a signature, macOS refuses to start a downloaded app until the user right-clicks it
and chooses **Open**, and some versions say the app "is damaged and can't be opened", which
is alarming and untrue. With a Developer ID signature and a notarization ticket, it opens
the first time with no warning.

This needs a paid Apple Developer account. The release workflow skips signing when the
secrets below are absent, so the build still works without one — it is just ad-hoc signed.

## What to create

### 1. A Developer ID Application certificate

In Xcode, **Settings → Accounts → Manage Certificates → + → Developer ID Application**, or
through [developer.apple.com](https://developer.apple.com/account/resources/certificates).
Then export it from **Keychain Access** as a `.p12` with a password, and base64 it:

```sh
base64 -i DeveloperID.p12 | pbcopy
```

The identity's name is the line that `security find-identity -v -p codesigning` prints, in
full, for example `Developer ID Application: Your Name (TEAMID1234)`.

### 2. An App Store Connect API key

Notarization uses an API key rather than a password. At
[appstoreconnect.apple.com](https://appstoreconnect.apple.com/access/integrations/api),
create a key with the **Developer** role and download the `.p8`. Apple lets you download it
once.

```sh
base64 -i AuthKey_XXXXXXXXXX.p8 | pbcopy
```

The **Key ID** is in the key's row. The **Issuer ID** is above the table.

## Repository secrets

Add these under **Settings → Secrets and variables → Actions**:

| Secret | What it holds |
|---|---|
| `MACOS_CERTIFICATE_P12` | the base64 of the `.p12` |
| `MACOS_CERTIFICATE_PASSWORD` | the password set when exporting it |
| `MACOS_SIGN_IDENTITY` | `Developer ID Application: Your Name (TEAMID1234)` |
| `MACOS_NOTARY_KEY_P8` | the base64 of the `.p8` |
| `MACOS_NOTARY_KEY_ID` | the key's ID |
| `MACOS_NOTARY_ISSUER_ID` | the issuer UUID |

Set either all six or none. With the first three only, the app is signed but not notarized,
which Gatekeeper still refuses on a machine that has not seen it before.

## Checking a build

On a Mac that has never run it, with the zip freshly downloaded:

```sh
spctl --assess --type execute --verbose=4 SolidLauncher.app
xcrun stapler validate SolidLauncher.app
codesign --verify --strict --deep --verbose=2 SolidLauncher.app
```

`spctl` printing `accepted` and `source=Notarized Developer ID` is the goal. Testing on the
machine that built it proves nothing: that machine already trusts the certificate.

## What this does not cover

- **SolidCraft itself.** Its disk image is published by `tangency-labs/solidcraft`, ad-hoc
  signed and not notarized, which is why SolidLauncher clears the quarantine flag after
  checking the download's SHA-256. Signing SolidLauncher does not change that. Notarizing
  SolidCraft's own release would, and would be the better fix.
- **Windows.** SmartScreen needs an Authenticode certificate, which is bought separately. An
  OV certificate still warns until the binary has been downloaded enough times to earn
  reputation; an EV certificate does not.
