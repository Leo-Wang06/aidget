# Security policy

## Trust boundary

aidget runs inside the local user's account. The user's home directory,
`~/.aidget`, the widgets the user installs, the Agents they run and the files
those processes can already write are inside the same trust boundary.

Reports that require prior local write access are not aidget vulnerabilities
unless they show that aidget grants access the attacker did not already have or
crosses an operating-system privilege boundary.

Widgets are user-controlled content, not trusted extensions. A widget can render
HTML and CSS, and can run JavaScript only when `permissions` includes `script`. The
widget iframe is sandboxed without same-origin access; it must not be able to
read another widget's files, reach Tauri IPC, or control the aidget shell.

The `network` list grants access to exact HTTPS origins. An empty list blocks
remote network access; paths, wildcards and non-HTTPS origins are rejected.

## In scope

- Escaping the widget sandbox or reading another widget's files.
- Reaching Tauri IPC or privileged host APIs from a widget.
- Bypassing the bridge's origin, host or path checks.
- Bypassing a widget's declared script or network permissions.
- Causing aidget to expose Agent configuration, credentials or unrelated local
  files.
- A malicious import link, Agent integration or bundled resource that gains
  privileges beyond the user's explicit action.
- Compromise of the release workflow or published artifacts.
- An operating-system privilege-boundary crossing caused by aidget.

## Out of scope

- Malicious widget HTML, CSS or JavaScript that the user chose to install or
  explicitly enabled with the `script` permission.
- Prompt injection in widget content, Agent instructions, repositories, logs or
  model output.
- Behavior of untrusted Agents, MCP servers, skills, packages or other tools the
  user installs.
- Changes made by someone who can already write to `~/.aidget`, Agent configs or
  the user's shell environment.
- Resource exhaustion caused by trusted local configuration or deliberately
  weakened settings.
- Exposing a local aidget bridge directly to the public internet.
- Reports that only show expected behavior from a user-approved action.

## Reporting a vulnerability

Do not open a public Issue. Report privately through GitHub Security Advisories:

1. Open the repository's **Security** tab.
2. Choose **Advisories**.
3. Create a private report.

Include:

- the affected aidget version or commit;
- the platform and WebView;
- reproduction steps or a proof of concept;
- the demonstrated impact and security-boundary crossing;
- any known mitigation.

Reports are reviewed before public disclosure.
