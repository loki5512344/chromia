<div align="center">

# Chromia

Music player for Arch Linux / Hyprland: local library, YouTube and SoundCloud streams via yt-dlp, dynamic album-art colors and a customizable interface.

![Rust](https://img.shields.io/badge/Rust-2024-black?style=flat-square&logo=rust&logoColor=white)
![GTK4](https://img.shields.io/badge/GTK4_Libadwaita-1A1A1A?style=flat-square&logo=gtk&logoColor=white)
![tokio](https://img.shields.io/badge/tokio-1E88E5?style=flat-square&logo=rust&logoColor=white)
![rodio](https://img.shields.io/badge/rodio-00897B?style=flat-square&logo=rust&logoColor=white)
![yt-dlp](https://img.shields.io/badge/yt--dlp-FF3D00?style=flat-square&logo=youtube&logoColor=white)
![SQLite](https://img.shields.io/badge/SQLite-003B57?style=flat-square&logo=sqlite&logoColor=white)
![license](https://img.shields.io/badge/license-GPLv3-blue?style=flat-square&logo=gnu&logoColor=white)
![status](https://img.shields.io/badge/status-development-yellow?style=flat-square)

[English](#english) | [Русский](#russian)

</div>

---

> **Note:** Chromia is in active development. API and config may change.

<a name="english"></a>

## English

### Overview

Chromia is a desktop music player for Linux (Wayland / Hyprland). It plays local files, streams from YouTube and SoundCloud through yt-dlp, extracts dynamic colors from album art, and lets you download any stream into your library.

### Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Language | Rust 2024 | Speed and safety |
| UI | GTK4 + Libadwaita | Native Wayland |
| Audio | rodio + symphonia | mp3/flac/ogg/opus/aac/wav playback |
| Tags | lofty | Metadata and cover art |
| Database | SQLite (rusqlite, bundled) | Library, playlists, history |
| Colors | color-thief | Dominant color from album art |
| HTTP | reqwest (rustls) | Lyrics API, covers |
| Lyrics | lrclib API | Synced lyrics |
| Streams | yt-dlp | YouTube Music / SoundCloud |
| Integrations | zbus (MPRIS2), discord-presence | Media keys, waybar, Discord |

### Sources

| Source | Status |
|--------|--------|
| Local | done |
| YouTube | done |
| SoundCloud | done |
| Chromia Server | planned |
| Spotify | planned |

### Features

| Feature | Description |
|---------|-------------|
| Local playback | Queue with shuffle/repeat, seek, volume |
| Streaming | YouTube and SoundCloud search + cache streaming |
| Download manager | Async yt-dlp downloads, progress, queue, cancel, 3 parallel |
| Library | Folder scanning, metadata, covers, search |
| Playlists & history | Persisted in SQLite, play-count tracking |
| Dynamic theme | Colors from album art (catppuccin fallback) or custom TOML |
| Synced lyrics | Current-line highlight and auto-scroll |
| MPRIS2 | Media keys, waybar, eww |
| Discord RPC | Rich presence |
| Onboarding | First-run screen: music folder, theme, integrations |
| Single-screen UI | Player bar + resizable Library / Lyrics / Queue columns |

### Development

```bash
cargo build                       # build
cargo run                         # run
cargo test                        # tests
cargo clippy --all-targets -- -D warnings   # lint
```

### Architecture

```
src/                    -- Rust backend
├── config/             -- serde + toml, ~/.config/chromia/config.toml
├── audio/              -- rodio engine on a dedicated thread, queue, equalizer
├── library/            -- scanner, SQLite (tracks, playlists, history), metadata
├── sources/            -- local, youtube, soundcloud (yt-dlp)
├── download/           -- download manager (progress, cancel, concurrency)
├── theme/              -- catppuccin, dynamic (color-thief), CSS generation
├── lyrics/             -- lrclib API
├── integrations/       -- MPRIS2 (zbus), Discord Rich Presence
└── ui/                 -- onboarding, window, widgets

assets/
└── style/
    └── base.css        -- static part of the theme
```

### Requirements

- Arch Linux or any Linux with Wayland / X11
- `gtk4`, `libadwaita` (development packages to build)
- `yt-dlp` for streaming (YouTube, SoundCloud)
- SQLite is bundled

---

<a name="russian"></a>

## Русский

### Обзор

Chromia - десктопный музыкальный плеер для Linux (Wayland / Hyprland). Проигрывает локальные файлы, стримит с YouTube и SoundCloud через yt-dlp, извлекает динамические цвета из обложки и позволяет скачать любой стрим в свою библиотеку.

### Стек

| Слой | Технология | Зачем |
|------|------------|-------|
| Язык | Rust 2024 | Скорость и безопасность |
| UI | GTK4 + Libadwaita | Нативный Wayland |
| Аудио | rodio + symphonia | Воспроизведение mp3/flac/ogg/opus/aac/wav |
| Теги | lofty | Метадата и обложки |
| БД | SQLite (rusqlite, bundled) | Библиотека, плейлисты, история |
| Цвета | color-thief | Доминантный цвет из обложки |
| HTTP | reqwest (rustls) | API текстов, обложки |
| Тексты | lrclib API | Синхронизированные тексты |
| Стримы | yt-dlp | YouTube Music / SoundCloud |
| Интеграции | zbus (MPRIS2), discord-presence | Медиа-клавиши, waybar, Discord |

### Источники

| Источник | Статус |
|----------|--------|
| Local | готово |
| YouTube | готово |
| SoundCloud | готово |
| Chromia Server | планируется |
| Spotify | планируется |

### Возможности

| Возможность | Описание |
|-------------|----------|
| Воспроизведение | Очередь с shuffle/repeat, seek, громкость |
| Стриминг | Поиск по YouTube и SoundCloud + кеш-стриминг |
| Менеджер загрузок | Асинхронные yt-dlp загрузки, прогресс, очередь, отмена, 3 параллельно |
| Библиотека | Сканирование папок, метадата, обложки, поиск |
| Плейлисты и история | Хранятся в SQLite, счётчик прослушиваний |
| Динамическая тема | Цвета из обложки (catppuccin fallback) или свой TOML |
| Тексты | Подсветка текущей строки и авто-скролл |
| MPRIS2 | Медиа-клавиши, waybar, eww |
| Discord RPC | Rich presence |
| Онбординг | Экран первого запуска: папка, тема, интеграции |
| Один экран | Плеер-бар + ресайзбельные колонки Библиотека / Текст / Очередь |

### Разработка

```bash
cargo build                       # сборка
cargo run                         # запуск
cargo test                        # тесты
cargo clippy --all-targets -- -D warnings   # линтер
```

### Архитектура

```
src/                    -- Rust-бэкенд
├── config/             -- serde + toml, ~/.config/chromia/config.toml
├── audio/              -- rodio-движок на отдельном потоке, очередь, эквалайзер
├── library/            -- сканер, SQLite (треки, плейлисты, история), метадата
├── sources/            -- local, youtube, soundcloud (yt-dlp)
├── download/           -- менеджер загрузок (прогресс, отмена, параллелизм)
├── theme/              -- catppuccin, dynamic (color-thief), генерация CSS
├── lyrics/             -- lrclib API
├── integrations/       -- MPRIS2 (zbus), Discord Rich Presence
└── ui/                 -- onboarding, window, widgets

assets/
└── style/
    └── base.css        -- статичная часть темы
```

### Требования

- Arch Linux или любой Linux с Wayland / X11
- `gtk4`, `libadwaita` (пакеты для разработки)
- `yt-dlp` для стримов (YouTube, SoundCloud)
- SQLite идёт bundled

---

### Links

- [Releases](../../releases)
- [Issues](../../issues)
- [License](LICENSE)

### License

GNU General Public License v3.0
