# 🎵 Chromia

> Музыкальный плеер для Arch Linux / Hyprland с динамическими цветами, мультисорсингом и скачиванием стримов

**Статус:** Проект в активной разработке. Базовый клиент работает (воспроизведение, библиотека, стримы, темы, интеграции). Архитектура кастомизации интерфейса (слоты, страницы, workspaces) — в стадии дизайна; сервер — в планах. Подробности — ниже.

---

## Концепция

Chromia — опенсорс десктопный музыкальный плеер на Rust + GTK4 + Libadwaita.
Интерфейс живёт вместе с музыкой: цвета меняются под обложку альбома, музыка приходит из локальных файлов, YouTube и SoundCloud, а понравившиеся стримы можно скачать в библиотеку. Chromia Server позволяет хранить всю коллекцию на своём сервере и слушать с любого устройства.

**Принципы:**
- Минимум RAM, нативный Wayland / Hyprland
- Catppuccin по дефолту, динамика из обложки опционально
- Конфиг в TOML — всё настраивается без перекомпиляции
- GTK никогда не блокируется: связь с аудио и сетью через `tokio::sync::mpsc`
- Опенсорс, GPL-3.0

---

## Стек

| Слой | Технология | Зачем |
|---|---|---|
| Язык | Rust 2024, MSRV 1.85 | Скорость, безопасность |
| UI | GTK4 + Libadwaita | Нативный Wayland |
| Аудио | rodio + symphonia | Воспроизведение mp3/flac/ogg/opus/aac/wav |
| Теги | lofty | Метадата и обложки |
| БД | SQLite (rusqlite, bundled) | Треки, плейлисты, история |
| Цвета | color-thief | Доминантный цвет из обложки |
| Конфиг | toml (serde) | Настройки пользователя |
| HTTP | reqwest (rustls) | API lyrics, обложки, сервер |
| Lyrics | lrclib API | Синхронизированные тексты |
| Стримы | yt-dlp (subprocess) | YouTube Music / SoundCloud |
| Сервер | axum + tokio | Chromia Server (self-hosted) |
| Интеграции | zbus (MPRIS2), discord-presence | Медиа-клавиши, waybar, Discord |

---

## Статус функций

### ✅ Реализовано
- **Воспроизведение:** локальные файлы, очередь с shuffle/repeat (off/queue/track), seek, громкость, удалённые стримы (скачивание в кеш через yt-dlp), gapless, ReplayGain, crossfade
- **Библиотека:** сканирование папок + авто-пересканирование по изменению (watch/inotify), SQLite-хранилище, метадата + обложки (lofty), поиск, плейлисты и история (DB API), счётчик прослушиваний, браузер по артистам/жанрам
- **Стримы:** YouTube (поиск + скачивание), SoundCloud (поиск + скачивание)
- **Менеджер загрузок:** асинхронные загрузки через yt-dlp, прогресс, очередь, отмена, до 3 параллельно
- **Удалённые метаданные:** `fetch_info` для одиночной ссылки (артист, длительность, обложка), кеширование обложек
- **Темы:** dynamic (цвета из обложки, включая удалённые), catppuccin (4 флейвора, 14 акцентов), custom TOML
- **Lyrics:** lrclib API, подсветка текущей строки, авто-скролл
- **Эквалайзер:** 10 полос, пресеты (Flat, Bass Boost, Vocal, Treble Boost, Rock, Jazz), реальный DSP (biquad-цепочка как rodio Source)
- **Визуализатор:** живой спектр-анализатор (бары) в правой панели
- **Интеграции:** MPRIS2 D-Bus сервис (media keys, waybar, eww), Discord Rich Presence
- **UI:** один цельный экран (плеер-бар + колонки Библиотека | Текст | Очередь), ресайзбельные колонки, онбординг при первом запуске
- **Онбординг:** при первом запуске — приветствие, выбор папки с музыкой (или по умолчанию `~/Music`), быстрые настройки (тема, громкость, MPRIS/Discord)

### 🔜 Планируется
- **Стримы:** Spotify через librespot, кеширование стримов с умной очисткой
- **UI:** настройки в GUI (полноценные), настраиваемые страницы, workspaces, drag-and-drop по слотам, мини-плеер, блюр обложки за окном, анимации смены цвета
- **Lyrics:** AI-транскрипция из аудио (whisper.cpp)
- **Сервер:** Chromia Server — self-hosted хранилище коллекции
- **Интеграции:** Last.fm scrobbling
- **Упаковка:** AUR-пакет, systemd unit

---

## Источники музыки

```
Local           →  папка с файлами, сканирование, метадата, обложки
YouTube         →  yt-dlp: поиск (ytsearch), стрим в кеш, скачивание в библиотеку
SoundCloud      →  yt-dlp: поиск (scsearch), стрим в кеш, скачивание в библиотеку
Chromia Server  →  self-hosted: REST API + streaming, синхронизация библиотеки
Spotify         →  через librespot (планируется)
```

Приоритет источника задаётся в конфиге:

```toml
[sources]
enabled = ["local", "youtube", "soundcloud", "chromia_server"]
default = "local"

[sources.local]
paths = ["~/Music"]
watch = false

[sources.youtube]
quality = "best"  # best | 320k | 256k | 128k
cache_dir = "~/.cache/chromia/youtube"

[sources.chromia_server]
url  = "http://192.168.1.100:7171"
token = "your-api-token"          # генерируется при первом запуске сервера
sync_library = true               # синхронизировать коллекцию с сервером
cache_art    = true               # кешировать обложки локально
```

---

## Chromia Server

Self-hosted сервер для хранения и стриминга музыкальной коллекции. Написан на Rust (axum), работает на любом Linux-сервере или NAS.

### Возможности
- Хранение и стриминг музыки по сети (HTTP range requests)
- REST API совместимый с клиентом Chromia
- Веб-интерфейс для управления коллекцией (опционально)
- Синхронизация метаданных и обложек
- Несколько пользователей с токенами
- Транскодирование на лету (opus/mp3/flac) под разные скорости
- Сканирование папок на сервере

### Быстрый старт

```bash
# скачать бинарник
curl -L https://github.com/loki/chromia/releases/latest/download/chromia-server -o chromia-server
chmod +x chromia-server

# запустить
./chromia-server --music-dir /mnt/nas/music --port 7171

# или через Docker
docker run -d \
  -p 7171:7171 \
  -v /mnt/nas/music:/music \
  -v /etc/chromia-server:/config \
  ghcr.io/loki/chromia-server:latest
```

### Конфиг сервера (`/etc/chromia-server/config.toml`)

```toml
[server]
port      = 7171
music_dir = "/mnt/nas/music"
data_dir  = "/var/lib/chromia-server"

[auth]
# токены генерируются командой: chromia-server token create <name>
enabled = true

[transcoding]
enabled  = true
formats  = ["opus", "mp3", "flac"]
bitrates = ["128k", "256k", "320k", "lossless"]

[scanning]
auto_scan = true
interval_minutes = 60
```

### systemd unit

```ini
[Unit]
Description=Chromia Music Server
After=network.target

[Service]
ExecStart=/usr/bin/chromia-server
Restart=on-failure
User=chromia

[Install]
WantedBy=multi-user.target
```

```bash
systemctl enable --now chromia-server
```

### API (основные эндпоинты)

```
GET  /api/tracks              # список треков
GET  /api/tracks/:id/stream   # стрим аудио (range requests)
GET  /api/tracks/:id/art      # обложка
GET  /api/search?q=...        # поиск
GET  /api/albums              # альбомы
GET  /api/artists             # артисты
POST /api/playlists           # создать плейлист
GET  /api/playlists/:id       # треки плейлиста
```

---

## Система тем

Иерархия: `dynamic` (цвета из обложки) → `catppuccin` → `custom` (свои hex).

```toml
[theme]
mode           = "dynamic"   # dynamic | catppuccin | custom
transition_ms  = 300
blur_background = true
blur_strength  = 20

# Смешивание: 70% catppuccin + 30% акцент из обложки
# Так интерфейс остаётся узнаваемым даже с яркими обложками
dynamic_mix = 0.3            # 0.0 = только catppuccin, 1.0 = только обложка

[theme.catppuccin]
flavor = "mocha"
accent = "mauve"

[theme.custom]
background = "#1e1e2e"
surface    = "#313244"
accent     = "#cba6f7"
text       = "#cdd6f4"
```

Как работает `dynamic`: обложка → color-thief извлекает палитру → самый насыщенный цвет становится accent → смешивается с catppuccin по `dynamic_mix` → фон/поверхности затемняются HSL-сдвигом → плавная анимация 300ms при смене трека → fallback на catppuccin если нет обложки.

---

## Glass UI

Отдельный режим внешнего вида с полупрозрачностью и блюром.
Работает только если композитор поддерживает blur (Hyprland, KDE).

```toml
[appearance]
glass         = false        # включить Glass UI
glass_opacity = 0.82
blur          = 24
noise         = true         # лёгкий шум для глубины
glass_mode    = "light"      # light | strong | disabled

# фон просвечивает сквозь стекло (рабочий стол / обложка)
follow_wallpaper = false
glass_background = "dynamic" # dynamic | solid | wallpaper

border_radius = 14           # радиус скруглений (8–20 px)
animations    = true         # отключить все анимации
```

Варианты `glass_background`:
- **dynamic** — динамический цвет из обложки применяется к стеклу;
- **wallpaper** — размывается обложка/рабочий стол, цвет берётся из него;
- **solid** — только текущая палитра, без просвечивания.

---

## UI — Архитектура интерфейса

### Философия

Интерфейс разделён на **фиксированный каркас** и **настраиваемые области**.
Пользователь не может случайно сломать интерфейс — кастомизация работает только внутри слотов.

```
┌─────────────┬───────────────────────────────┬──────────────────┐
│  Sidebar    │  Center (страницы)            │  Right Panel     │
│  (фикс)     │  (фикс)                       │  (настраивается) │
├─────────────┴───────────────────────────────┴──────────────────┤
│  Bottom Player (настраивается)                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Фиксированные области

**Sidebar** — навигация, плейлисты, пользователь. Не двигается.

**Center** — контент текущей страницы:
- Library, Search, Artist, Album, Playlist

Эти области никогда не меняют положение.

### Настраиваемая правая панель (Slots)

Правая панель состоит из вертикальных **слотов**. Виджеты переставляются между слотами drag-and-drop прямо в UI — конфиг обновляется автоматически. И наоборот: можно задать порядок в TOML и он применится в интерфейсе.

**Доступные виджеты:**

| Виджет | Описание |
|---|---|
| AlbumArt | Большая обложка с тенью |
| Player | Прогресс-бар + контролы |
| Lyrics | Синхронизированный текст |
| Queue | Очередь воспроизведения |
| Equalizer | 10-полосный эквалайзер |
| Visualizer | Аудио-визуализация |
| AlbumInfo | Год, жанр, лейбл |
| ArtistInfo | Биография артиста |
| SimilarAlbums | Похожие альбомы |
| AudioInfo | Битрейт, codec, sample rate |
| Devices | Выбор аудио-выхода |

**Конфиг слотов:**

```toml
[layout.right_panel]
slots = ["AlbumArt", "Player", "Lyrics", "Queue"]

# каждая страница может хранить свой лейаут
[layout.pages.album]
slots = ["AlbumArt", "AlbumInfo", "Player", "SimilarAlbums"]

[layout.pages.artist]
slots = ["ArtistInfo", "Player", "Queue"]
```

### Настраиваемый Bottom Player

```toml
[layout.bottom_player]
# minimal | default | audiophile | custom
preset = "default"

# или задать вручную
elements = ["Cover", "Song", "Artist", "Progress", "Controls", "Volume"]
```

Пресеты:
- **minimal** — `Cover Song Play`
- **default** — `Cover Song Artist Progress Controls Volume`
- **audiophile** — `Cover Song Artist Waveform Bitrate SampleRate Codec Device Progress Controls Volume`

### Пресеты интерфейса

Chromia поставляется с готовыми пресетами лейаута:

| Пресет | Описание |
|---|---|
| Default | Рекомендуемый интерфейс |
| Minimal | Максимум свободного пространства |
| Lyrics | Большая область текста |
| Audiophile | Максимум информации (битрейт, codec, waveform) |
| Compact | Для небольших экранов |

Пресеты можно экспортировать/импортировать как TOML-файлы и шарить с другими.

---

## UI — настраиваемые страницы

Кастомизация не ограничивается правой панелью. **Каждая страница** (Library, Album, Artist, Search, Playlist) конфигурируема: свой набор виджетов для центра и правой панели.

```toml
[layout.pages.library]
center = ["AlbumsGrid", "Filters"]
right  = ["Queue", "Lyrics"]

[layout.pages.album]
center = ["Cover", "AlbumInfo", "Credits"]
right  = ["SimilarAlbums", "Player"]

[layout.pages.artist]
center = ["ArtistInfo", "Albums", "TopTracks"]
right  = ["Queue"]

[layout.pages.search]
center = ["Filters", "Results"]
right  = ["History", "Recommendations"]
```

Так каждый раздел выглядит по-своему, но интерфейс остаётся предсказуемым.

---

## UI — состояния виджетов

Виджеты имеют состояния, чтобы не раздувать интерфейс кнопками:

```toml
[layout.right_panel.slots.AlbumArt]
mode = "compact"       # compact | large | hidden

[layout.right_panel.slots.Lyrics]
mode = "fullscreen"    # collapsed | normal | fullscreen

[layout.right_panel.slots.Queue]
mode = "collapsed"     # collapsed | normal
```

Пример: `Lyrics mode = "fullscreen"` на маленьком экране — текст на весь центр; `Queue mode = "collapsed"` — только компактная шапка до смены трека.

---

## UI — режим редактирования (Layout Edit Mode)

Чтобы случайно не сломать интерфейс во время прослушивания, кастомизация включается явно:

```toml
[appearance]
edit_mode = false
```

Когда `edit_mode = true`:
- у слотов и панелей появляются рамки и заголовки;
- видны места сброса (drop zones) между слотами;
- виджеты перетаскиваются drag-and-drop;
- конфиг обновляется автоматически при перестановке;
- после выключения всё снова выглядит как обычный плеер.

Никакого случайного перетаскивания — редактирование всегда осознанное.

---

## UI — Workspaces

Одной кнопкой (или хоткеем) меняется весь интерфейс: набор виджетов, страниц и нижний плеер.

```toml
[workspaces.casual]
right = ["Lyrics", "Queue"]

[workspaces.audiophile]
bottom_player = "audiophile"
right = ["Spectrum", "Waveform", "Bitrate", "Devices"]

[workspaces.coding]
bottom_player = "minimal"
right = ["Controls"]

active = "casual"
```

| Workspace | Для кого |
|---|---|
| Casual | Повседневное прослушивание: текст + очередь |
| Audiophile | Максимум информации: waveform, битрейт, устройства |
| Coding | Минимум интерфейса: только контролы |
| Party | Визуализатор на весь центр |

Workspaces комбинируются с пресетами лейаута и состояниями виджетов — можно переключаться между полностью разными интерфейсами, не трогая конфиг руками.

---

## Структура проекта

```
chromia/
├── Cargo.toml
├── Cargo.workspace.toml       # воркспейс: client + server
├── config/
│   └── default.toml
├── assets/
│   └── style/
│       └── base.css
├── chromia-client/            # десктопный плеер
│   └── src/
│       ├── main.rs
│       ├── app.rs
│       ├── config/
│       │   ├── mod.rs
│       │   └── schema.rs
│       ├── audio/
│       │   ├── mod.rs
│       │   ├── player.rs
│       │   ├── queue.rs
│       │   └── equalizer.rs
│       ├── library/
│       │   ├── mod.rs
│       │   ├── metadata.rs
│       │   ├── database.rs
│       │   └── scanner.rs
│       ├── sources/
│       │   ├── mod.rs
│       │   ├── local.rs
│       │   ├── youtube.rs
│       │   ├── soundcloud.rs
│       │   └── chromia_server.rs   # клиент к Chromia Server
│       ├── download/
│       │   └── mod.rs
│       ├── theme/
│       │   ├── mod.rs
│       │   ├── catppuccin.rs
│       │   ├── dynamic.rs
│       │   └── css.rs
│       ├── lyrics/
│       │   └── lrclib.rs
│       ├── integrations/
│       │   ├── mpris.rs
│       │   └── discord.rs
│       └── ui/
│           ├── mod.rs
│           ├── onboarding.rs
│           ├── window.rs
│           ├── layout.rs           # слоты, drag-and-drop, синк с конфигом
│           └── widgets/
│               ├── player.rs
│               ├── library.rs
│               ├── lyrics.rs
│               ├── queue.rs
│               ├── equalizer.rs
│               ├── album_art.rs
│               ├── audio_info.rs
│               └── visualizer.rs
└── chromia-server/            # self-hosted сервер
    └── src/
        ├── main.rs
        ├── config.rs
        ├── api/
        │   ├── mod.rs
        │   ├── tracks.rs
        │   ├── albums.rs
        │   ├── artists.rs
        │   ├── playlists.rs
        │   └── search.rs
        ├── scanner.rs
        ├── database.rs
        ├── transcoding.rs         # opus/mp3/flac на лету
        └── auth.rs                # токены
```

---

## Роадмап

### ✅ v0.1 — MVP (готово)
- Воспроизведение локальных файлов
- Базовый UI: плеер-бар + Библиотека + Очередь
- Catppuccin + custom темы, TOML-конфиг
- Онбординг первого запуска

### ✅ v0.2 — Темы и стримы (готово)
- Динамические цвета из обложки (включая удалённые)
- Lyrics через lrclib с подсветкой
- YouTube / SoundCloud: поиск + стрим через yt-dlp
- Менеджер загрузок с прогрессом и отменой

### ✅ v0.3 — Интеграции и данные (готово)
- Эквалайзер (модель + пресеты)
- MPRIS2, Discord Rich Presence
- Плейлисты и история в БД
- Удалённые метаданные и кеш обложек

### 🔜 v1.0 — Полировка (в работе)
- ✅ Эквалайзер DSP (biquad), ReplayGain, crossfade, визуализатор
- Настройки в GUI
- ✅ Браузер библиотеки: артисты/жанры, плейлисты в UI
- ✅ Авто-обновление библиотеки (watch/poll), кеш-менеджер стримов
- Настраиваемый лейаут правой панели (слоты + drag-and-drop)
- ✅ Glass UI режим
- AUR-пакет

### 🔜 v1.1 — Сервер
- **Chromia Server** — self-hosted хранилище коллекции
- REST API + HTTP range streaming
- Docker образ
- Транскодирование на лету
- Несколько пользователей с токенами
- Синхронизация клиента с сервером

### 🔜 v1.2+
- Spotify (librespot)
- Last.fm scrobbling
- AI lyrics (whisper.cpp)
- Мини-плеер
- Веб-интерфейс для Chromia Server
- Мобильный клиент (?)
- Плагины / расширения

---

## Установка

```bash
# AUR (планируется)
yay -S chromia-bin

# из исходников
git clone https://github.com/loki/chromia
cd chromia
cargo build --release -p chromia-client

# сервер
cargo build --release -p chromia-server
```

---

## Зависимости системы

```
gtk4 libadwaita    # UI
yt-dlp             # стримы (YouTube, SoundCloud)
```

SQLite идёт bundled (rusqlite), системный GStreamer не нужен.

---

*Chromia — твой плеер, твои цвета, твоя музыка.*
