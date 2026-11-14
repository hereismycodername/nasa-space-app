# Запуск, Raspberry Pi и настоящий датчик

На компьютере и Raspberry Pi приложение состоит из одного Rust-сервера, SQLite
и собранного React `dist/`. ESP32, Arduino и Pico передают измерения серверу.
Native GUI из `ui/`, OpenGL и Vite для production не нужны. Аппаратный запуск
на Pi и конкретных датчиках пока **не проверен**; проверены программные пути.

## Быстрое демо на компьютере

Из корня проекта, после установки Node из `.nvmrc` и Rust stable:

```sh
npm ci
npm run doctor
npm run demo
```

`demo` собирает React и Rust, запускает сервер, ждёт `/api/health`, затем включает
синтетический пульс. Открыть `http://127.0.0.1:8080`. Ctrl+C останавливает сервер
и симулятор; прерываются также запросы и ожидания повторов. Cargo ищется в PATH
и `~/.cargo/bin`. Ошибка сборки или занятый порт останавливают запуск с пояснением.
Автоматической установки инструментов, очистки БД и загрузки `.env` нет.

```sh
SIM_COUNT=3 SIM_INTERVAL_MS=100 npm run demo -- --no-build
npm run start
```

Первая команда использует готовую сборку, подтверждает три записи и завершает
все процессы. `start` запускает существующую debug-сборку с React без симулятора.
После обычного `demo` путь последнего Cargo executable запоминается в target-каталоге,
включая настроенный build target. Для другого готового бинарника можно задать
`SERVER_BINARY=/absolute/path/space-health-server npm run start`; в этом режиме
Cargo не нужен. При смене target без новой сборки укажите SERVER_BINARY явно.
Обе команды используют прежнюю БД; для изолированного опыта задайте новый
`DATABASE_URL=sqlite://demo-test.db`. Для разработки остаются отдельные команды
`npm run server`, `npm run dev`, `npm run simulator`.

Симулятор отдельно принимает `API_URL=http://<сервер>:8080` или тот же URL с `/api`;
дублирования `/api/api` не возникает. Другие пути/query запрещены. `SIM_COUNT=0`
означает непрерывную работу, `SIM_INTERVAL_MS` минимум 100, `SIM_RETRY_MS` задаёт
паузу повторов, `SIM_REQUEST_TIMEOUT_MS` — timeout (100…60000 ms, default 5000).
Во время повторов sessionId/sequence/payload не меняются. Симулятор хранит один
неподтверждённый пакет только в памяти; после перезапуска создаёт новую сессию.
В `demo` API_URL симулятора задаётся автоматически по BIND_ADDR собственного сервера.

## Raspberry Pi OS 64-bit

Целевая среда — Raspberry Pi 3/4/5 с 64-bit ОС (`uname -m`: `aarch64`).
Для сервера подходит Raspberry Pi OS Lite без графического рабочего стола.
[Официальная документация Raspberry Pi](https://www.raspberrypi.com/documentation/computers/os.html)
описывает различия Lite/desktop и 32/64-bit образов. Не переустанавливайте рабочую
карту ради этого примера: сначала сохраните данные и проверьте текущую систему.

Установите Rust stable через [официальный rustup](https://rustup.rs/) и Node 24.x
по `.nvmrc` через [официальные варианты установки Node](https://nodejs.org/en/download).
Пакеты сборки для Debian/Pi OS устанавливаются администратором:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config python3 python3-venv
uname -m
npm run doctor
npm ci
npm run build
CARGO_BUILD_JOBS=1 cargo build --release --locked --manifest-path server/Cargo.toml --bins
BIND_ADDR=0.0.0.0:8080 FRONTEND_DIST=dist ./target/release/space-health-server
```

Открыть `http://<LAN-IP-Pi>:8080` с другого устройства в доверенной сети. `localhost`
в браузере телефона указывает на телефон. `0.0.0.0` — адрес прослушивания, а не адрес
для открытия страницы. Снаружи сети сервис не публикуем: авторизации/TLS пока нет.
Если порт нужен только локально, задайте `127.0.0.1:8080`.

`CARGO_BUILD_JOBS=1` уменьшает одновременную нагрузку сборки. На плате с малой RAM
сборка Node/Rust всё равно может завершиться OOM; диагностируйте `free -h`, `df -h`
и журнал ОС. Альтернатива — подготовить `dist` на ноутбуке, а Linux arm64-бинарники
собрать в совместимом Linux arm64 окружении. macOS/x86/Windows-бинарники не подходят.
На готовой Pi нужны только Rust-бинарник, `dist` и доступная для записи SQLite;
Node/Cargo нужны для сборки, а не для работы сервиса.

## Автозапуск systemd

Пример ожидает установку в `/opt/space-health`, пользователя `space-health`, данные
в `/var/lib/space-health`. Выполнить после успешного ручного запуска release-сборки:

```sh
sudo useradd --system --user-group --home-dir /var/lib/space-health --shell /usr/sbin/nologin space-health
sudo install -d /opt/space-health/bin /opt/space-health/dist
sudo install -m 755 target/release/space-health-server target/release/space-health-admin /opt/space-health/bin/
sudo cp -R dist/. /opt/space-health/dist/
sudo install -m 644 deploy/space-health.env.example /etc/space-health.env
sudo install -m 644 deploy/space-health.service /etc/systemd/system/space-health.service
sudo systemctl daemon-reload
sudo systemctl enable --now space-health
sudo systemctl status space-health
journalctl -u space-health -n 100 --no-pager
```

Если пользователь уже создан, шаг `useradd` пропускается. Это пример первой
установки; перед обновлением конфигурации сохраните свои значения. systemd задаёт
WorkingDirectory и абсолютные пути, создаёт каталог данных через StateDirectory,
запускает процесс без root. `EnvironmentFile` загружает переменные явно. Остановка:
`sudo systemctl stop space-health`. Существующий файл SQLite не удаляется.

## Источник настоящих измерений

Регистрация устройства выполняется локальным admin CLI, не через открытый HTTP:

```sh
cargo run --locked --manifest-path server/Cargo.toml --bin space-health-admin -- register-device sensor-01
cargo run --locked --manifest-path server/Cargo.toml --bin space-health-admin -- list-devices
```

CLI должен использовать тот же DATABASE_URL, что сервер. Для установленного сервиса:

```sh
sudo -u space-health env DATABASE_URL=sqlite:///var/lib/space-health/space-health.db /opt/space-health/bin/space-health-admin register-device sensor-01
```

Профиль — `demo-astronaut`, source — `sensor`; клиент не задаёт их. Повтор регистрации
того же sensor device идемпотентен. Уже занятые `manual-01`/`simulator-01` не превращаются
в sensor. Регистрация сама по себе не является аутентификацией.

### USB/UART или stdin → bridge → HTTP

Плата/драйвер передаёт UTF-8 NDJSON: один объект в строке, максимум 4096 bytes
вместе с переводом строки. Допустимы только metric/value/unit/measuredAt:

```json
{"metric":"temperature","value":36.6,"unit":"°C","measuredAt":null}
```

`measuredAt` — настоящий RFC3339 timestamp измерения с часовым поясом. При отсутствии
часов поле пропускается или равно null. Bridge не подставляет время получения.
Значение `36.6` выше — пример протокола, не показание реального подключённого датчика.

| metric | unit | Допустимый ввод |
|---|---|---|
| heartRate | bpm | > 0 |
| systolicPressure / diastolicPressure | mmHg | > 0 |
| temperature | °C | > 0 |
| sleepHours | h | 0…24 |
| mood | score | 1…5 |

Это границы API, не медицинские нормы. Для raw ADC/вольтажа нужны драйвер конкретного
датчика, преобразование и калибровка; универсальный bridge не выдаёт из ADC пульс или
температуру. Модель платы, сенсора и протокол пока неизвестны — прошивка не реализована.

Проверка stdin без сторонних Python-пакетов (синтетический пример):

```sh
printf '%s\n' '{"metric":"temperature","value":36.6,"unit":"°C","measuredAt":null}' | python3 -B tools/sensor_bridge.py --device-id sensor-01 --outbox sensor-outbox.db
```

USB/UART требует только дополнительный pyserial в отдельном окружении:

```sh
python3 -m venv .venv.local
.venv.local/bin/python -m pip install -r tools/requirements-serial.txt
.venv.local/bin/python -m serial.tools.list_ports
.venv.local/bin/python -B tools/sensor_bridge.py --device-id sensor-01 --outbox sensor-outbox.db --port /dev/serial/by-id/REPLACE_WITH_YOUR_DEVICE --baud 115200
```

Подставьте обнаруженный порт. На Linux права на serial обычно выдаются через группу
устройства (проверьте `ls -l`); не запускайте весь bridge под root. Скорость должна
совпадать с прошивкой. Read timeout сохраняет неполную строку до её завершения,
а не превращает её в измерение. См. [pySerial: ports, timeout и readline](https://pyserial.readthedocs.io/en/latest/shortintro.html).

Перед HTTP-запросом bridge атомарно сохраняет payload и sequence в SQLite outbox.
Сессия и счётчик переживают перезапуск. Пока одна запись не подтверждена, следующая
не считывается: для pipe это backpressure. Для UART без согласованного flow control
буфер платы/ОС может переполниться; используйте протокол подтверждений на стороне
производителя либо поддерживаемый и подключённый RTS/CTS (`--rtscts`). Bridge не
гарантирует сохранность байтов, которые ещё не прочитаны с устройства.

После сетевой ошибки/5xx повторяется **тот же** payload. 4xx (кроме 429) останавливает
bridge с ошибкой и оставляет запись в outbox. После исправления регистрации/сервера:

```sh
python3 -B tools/sensor_bridge.py --device-id sensor-01 --outbox sensor-outbox.db --drain-only
```

Не удаляйте outbox для обхода ошибки: в нём неподтверждённые данные. Повреждённый
payload/409 требует разбора сохранённой строки; автоматического отбрасывания нет.
Ctrl+C/SIGTERM прерывает ввод и retry; незавершённая запись остаётся для повтора.
Одна БД outbox предназначена одному устройству и одному bridge; для другого датчика
укажите другой файл. `.lock` предотвращает параллельный запуск на том же outbox.

### ESP с прямым HTTP

ESP может отправлять существующий `POST /api/measurements` напрямую по доверенной
LAN: deviceId зарегистрирован заранее, sessionId меняется на новую сессию, sequence
увеличивается для каждого нового измерения. До подтверждения 200/201 повторять тот
же ключ и payload, не присваивать новое время. При неизвестном времени — null.
Для переживания питания очередь должна храниться на устройстве. См. [контракт API](contracts.md).
Готового драйвера Wi-Fi/датчика или прошивки ESP этот репозиторий пока не содержит.

## Проверки без железа

```sh
npm run test:tools
npm run build
cargo build --locked --bins
npm run test:demo
```

Тесты проверяют stdin → HTTP, повтор одинаковых байтов, durable outbox, неизвестное
время, ошибки ввода и остановку retry. Demo smoke проверяет конечный симулятор,
очистку процессов, перезапуск сервера, сохранность данных и выдачу React. Эти проверки
не заменяют испытание конкретного датчика и Raspberry Pi.
