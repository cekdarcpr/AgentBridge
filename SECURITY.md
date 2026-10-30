# Security policy

agentBridge handles API keys for paid services, so security reports are taken seriously.

## Supported versions

Fixes are made on the latest release. Please check that a problem still exists in it before reporting.

## Reporting a vulnerability

Please **do not open a public issue** for a vulnerability. Report it privately through GitHub instead:
open the repository's **Security** tab and choose **Report a vulnerability**. Include the version
(`agb --version`), your operating system, the steps to reproduce it and the impact you expect.

You can expect a first reply within a week. Once a fix is released, the report is credited in the
changelog unless you prefer otherwise.

## What counts

Examples of what we want to hear about:

- An API key that ends up anywhere other than the operating system keychain, an environment variable
  you set, or the request header of its own provider: a log, the call database, an error message,
  JSON output, a URL or another host.
- A way to make `agb` write outside the path it was given, or to read a file outside the project
  through a style profile.
- A way to spend more than the configured limits allow.

## How keys are protected

- Keys are stored in the operating system keychain (Windows Credential Manager, macOS Keychain or the
  Secret Service on Linux), or read from the provider's usual environment variable.
- `agb keys set` reads the key from standard input, so it never appears in shell history.
- Keys are sent only as request headers to their own provider. Redirects are followed only within the
  same origin, so a key header can never be forwarded to another host.
- Keys are removed from every provider error message before it is shown or logged, and are never
  written to the call database.
