# Robloader (Tauri)

Réécriture de l'UI de Robloader avec un rendu **100 % natif** :

- **Windows 11** → matériau **Mica** (`apply_mica`)
- **macOS** → **Vibrancy** / `NSVisualEffectView` (`apply_vibrancy`)

Aucune fausse transparence CSS : la fenêtre est **frameless** + **transparente**, et le
`<body>` est en `rgba(0,0,0,0)` pour laisser le matériau natif de l'OS transparaître.

Stack : **Tauri v2** + **React + TypeScript + Vite**, plugin
[`window-vibrancy`](https://crates.io/crates/window-vibrancy).

## ⚠️ Plateformes

Mica et Vibrancy sont des matériaux **Windows 11 / macOS uniquement**. Sur **Linux**,
les appels sont volontairement no-op (cfg-gated) : la fenêtre s'affiche mais **sans le
matériau** — c'est normal. Le rendu « parfait » se constate sur un PC Windows 11 ou un Mac.

## Pièces clés

| Quoi | Où |
|------|----|
| Application du matériau natif | `src-tauri/src/lib.rs` (`setup`) |
| Fenêtre transparente / frameless | `src-tauri/tauri.conf.json` (`transparent`, `decorations`, `macOSPrivateApi`) |
| Permissions titlebar (drag + boutons) | `src-tauri/capabilities/default.json` |
| Fond transparent global | `src/styles.css` (`html, body { background-color: rgba(0,0,0,0) }`) |
| Titlebar custom + Hello World | `src/App.tsx`, `src/App.css` |

## Développement

```bash
npm install
npm run tauri dev      # lance Vite + la fenêtre Tauri
```

## Build

```bash
npm run tauri build    # produit l'app native pour l'OS courant
```

### Prérequis

- **Rust** (stable, via rustup)
- **Windows** : Visual Studio Build Tools + WebView2 (présent par défaut sur Win 11)
- **macOS** : Xcode Command Line Tools
- **Linux** (dev/test seulement, sans matériau) : `libwebkit2gtk-4.1-dev`,
  `libdbus-1-dev`, `librsvg2-dev`, `libayatana-appindicator3-dev`, `build-essential`,
  `pkg-config`
