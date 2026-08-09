# Publication fonts

These font files are vendored so the technical whitepaper paginates the same
way on every supported build host. They are documentation assets and are not a
runtime dependency of Kendr Optimizer.

## Inter and Space Grotesk

Source: the [Google Fonts repository](https://github.com/google/fonts) at commit
`2d85e20401920891efb7cd6272d6339685df2820`.

- `Inter-Regular.ttf`: static `opsz=14`, `wght=400` instance.
- `Inter-SemiBold.ttf`: static `opsz=14`, `wght=600` instance.
- `Inter-Italic.ttf`: static italic `opsz=14`, `wght=400` instance.
- `SpaceGrotesk-SemiBold.ttf`: static `wght=600` instance.

The static instances were generated from the upstream variable TTFs with
fontTools 4.63.0. See `Inter-OFL.txt` and `Space-Grotesk-OFL.txt`.

## Cascadia Mono

`CascadiaMono-Regular.ttf` is Microsoft Cascadia Code version `2102.025`,
corresponding to upstream tag `v2102.25` (commit
`911dc421f333e3b72b97381d16fee5b71eb48f04`). See
`Cascadia-Code-LICENSE.txt`.

## SHA-256

```text
0e141cb99609f6f10ad05313fd1807d5cc9e28658dcbb35ab162e52ff67dc718  CascadiaMono-Regular.ttf
b40812033f22a94c2cf5e677268fc8885753412ebd125fc2ae1f83a7502e8532  Inter-Italic.ttf
c6c2b34766876e1acb843b43b82b9ff484da43cfeba7ca626ba7573c09809978  Inter-Regular.ttf
76a22cc8882e1a3e70a11ae27e63468b1797f4493cd1bfe6b1a2c66531136ad8  Inter-SemiBold.ttf
df2e7b8ec10aabc3c47e326d85aeff6fe9609d50dab84a5d433eaa0aef3da792  SpaceGrotesk-SemiBold.ttf
```
