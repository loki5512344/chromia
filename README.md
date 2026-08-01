# Chromia

![Rust](https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white)
![GTK4](https://img.shields.io/badge/GTK4_Libadwaita-1A1A1A?style=flat-square&logo=gtk&logoColor=white)
![tokio](https://img.shields.io/badge/tokio-1E88E5?style=flat-square&logo=rust&logoColor=white)
![rodio](https://img.shields.io/badge/rodio-00897B?style=flat-square&logo=rust&logoColor=white)
![yt-dlp](https://img.shields.io/badge/yt--dlp-FF3D00?style=flat-square&logo=youtube&logoColor=white)
![SQLite](https://img.shields.io/badge/SQLite-003B57?style=flat-square&logo=sqlite&logoColor=white)
![status](https://img.shields.io/badge/status-development-yellow?style=flat-square)
![license](https://img.shields.io/badge/license-GPLv3-blue?style=flat-square)

Музыкальный плеер для Arch Linux / Hyprland: локальная библиотека, стримы с YouTube и SoundCloud через yt-dlp, динамические цвета из обложки и настраиваемый интерфейс.

## Стек

- **GTK4 + Libadwaita** - нативный Wayland-интерфейс
- **rodio + symphonia** - воспроизведение mp3/flac/ogg/opus/aac/wav
- **tokio** - async runtime
- **yt-dlp** - стримы и скачивание с YouTube / SoundCloud
- **rusqlite (bundled)** - локальная библиотека, плейлисты, история
- **lofty** - метадата и обложки
- **color-thief** - доминантный цвет из обложки
- **lrclib API** - синхронизированные тексты
- **zbus (MPRIS2)** + **discord-presence** - интеграции

## Источники

| Источник       | Статус       |
|----------------|--------------|
| Local          | реализован   |
| YouTube        | реализован   |
| SoundCloud     | реализован   |
| Chromia Server | планируется  |
| Spotify        | планируется  |

## Возможности

- Воспроизведение локальных файлов и стримов, очередь с shuffle/repeat, seek, громкость
- Менеджер загрузок: yt-dlp, прогресс, очередь, отмена, до 3 параллельных загрузок
- Поиск по локальной библиотеке и в YouTube / SoundCloud
- Плейлисты и история прослушивания (SQLite)
- Динамические цвета из обложки (catppuccin fallback) и custom-палитра через TOML
- Синхронизированные тексты с подсветкой текущей строки и авто-скроллом
- MPRIS2 (media keys, waybar, eww) и Discord Rich Presence
- Онбординг при первом запуске: выбор папки, темы, интеграций
- Один цельный экран: плеер-бар + ресайзбельные колонки (Библиотека / Текст / Очередь)

## Разработка

```bash
cargo build                       # сборка
cargo run                         # запуск
cargo test                        # тесты
cargo clippy --all-targets -- -D warnings   # линтер
```

## Архитектура

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

## Лицензия

GPL-3.0-only.
