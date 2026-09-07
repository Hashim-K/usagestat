# Native IDE discovery

Implementation: #12. Real-app/version qualification remains pending.

The host's existing `ls.discover(options)` still returns the discovered endpoint
or null. `ls.discoverStatus(options)` adds `status`, `reasonCode`, and `result`.
Possible statuses are `ready`, `missing`, `ambiguous`, `unavailable`, `unsupported`,
and `invalid`. Ready results retain `csrf`, `ports`, and `extensionPort`, adding
the selected `pid`. Tokens and process command lines are never logged.

Options retain `processName`, `markers`, `csrfFlag`, and `portFlag`; an optional
`pid` selects one running instance. Discovery requires one unambiguous instance,
matching executable identity and all provider markers. Duplicate flags,
empty/control-character tokens, zero/out-of-range ports and malformed argument
arrays are rejected. Multiple matches require a PID instead of choosing another
account or app arbitrarily. Antigravity IDE supports provider
`settings.ideProcessId` and reports actionable discovery failures.

Linux reads same-user `/proc` executable links and NUL-separated command-line
arguments. macOS lists same-user executable identities with `ps`, then obtains
the original argument array through `KERN_PROCARGS2`; it does not split the
displayed command line or consume environment entries as arguments. Windows
uses installed Windows PowerShell with no profiles and a bounded local CIM
`Win32_Process` query, checking `GetOwnerSid` against the current user. Only
matching same-user command lines are returned; WMIC and administrator rights
are not required. Windows argument decoding preserves quotes/backslashes and
literal shell characters. All helper output and execution time are bounded by
the shared process runner; cancellation removes the helper process tree.

Discovery does not read arbitrary process memory, elevate privileges, or claim
that a found process represents a verified upstream IDE version. Existing
provider code keeps endpoint requests on loopback. The legacy agy request with
no token/port flags continues to return no authenticated endpoint.

The native fixture builds a disposable language-server executable with spaces,
Unicode and quoted tokens, then makes a local authenticated HTTP request using
the discovered token. It tests multiple processes, explicit PID selection,
process exit, and the old discovery API. Pure fixtures test malformed flags and
all three argument encodings. Real IDEs, protected/denied process access and
minimum OS sessions remain part of release qualification.

Sources: [Win32_Process](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-process),
[GetOwnerSid](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/getownersid-method-in-class-win32-process),
[Get-CimInstance](https://learn.microsoft.com/en-us/powershell/module/cimcmdlets/get-ciminstance),
and [Apple's ps argument reader](https://github.com/apple-oss-distributions/adv_cmds/blob/main/ps/print.c).
These APIs
establish discovery mechanisms; they do not verify a proprietary IDE's flags.
