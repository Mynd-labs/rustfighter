# Street Fighter II (DOS, 1995) — Reverse Engineering Notes

Analysis of the U.S. Gold / Creative Materials MS-DOS port of Street Fighter II
(`street-fighter-ii/` release with `SF2.EXE` + `.SF2` data files).

All findings are derived from the shipped artifact with REA (Ghidra provider) and
offline hex analysis. Addresses are **linear Ghidra coordinates** (image base
`0x10000`); `file_offset = linear - 0xFFA0`. The game's data segment base is
linear `0x1AA40`, so data-segment references in code appear as immediates equal
to `linear - 0x1AA40`.

## Findings index

| Document | Scope |
|---|---|
| [engine-overview.md](findings/engine-overview.md) | Executable identity, memory map, runtime libraries, file inventory, indexed asset pipeline, string inventory, features |
| [sprite-format.md](findings/sprite-format.md) | Byte-level format of `SPRITE1.SF2` / `SPRITE2.SF2` banks |
| [general-sf2-formats.md](findings/general-sf2-formats.md) | `GENERAL.SF2` asset classes (fonts, audio, palettes, animations) and chain analysis |
| [config-highscores.md](findings/config-highscores.md) | `CONFIG.SF2` and `HIGHSCOR.SF2` field layouts |
| [asset-loader.md](findings/asset-loader.md) | Runtime asset directory, name lookup, I/O ABI, compression flag |

## Reproducing

```bash
# Ghidra provider (verified against 12.1.4)
export GHIDRA_INSTALL_DIR=/opt/ghidra_12.1.4_PUBLIC
export JAVA_HOME=/usr/local/sdkman/candidates/java/21.0.12+1-ms

# Analyze the executable (REA CLI)
rea decompile <game>/SF2.EXE <address> --format json
```

Game files are intentionally **not** committed to this repository.
