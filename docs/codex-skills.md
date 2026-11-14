# Навыки Codex для Space Health

Установлены в личный каталог Codex на этом компьютере. Это инструкции для агента,
не зависимости Rust/React. Установка не меняет runtime приложения и не
устанавливает навыки автоматически остальным участникам команды.

## Добавлены

| Навык | Источник | Применение |
|---|---|---|
| rust-best-practices | [Apollo GraphQL](https://github.com/apollographql/skills/tree/9dda1397730370aeb8df4ff8f2015c34c86b5003/skills/rust-best-practices) | Rust, ownership, ошибки, Clippy, тесты |
| vercel-react-best-practices | [Vercel](https://github.com/vercel-labs/agent-skills/tree/1a2c79d4e6c4a2283dc2c614b6538cbe1ed5ca62/skills/react-best-practices) | React, запросы, рендеринг, размер сборки |
| vitest | [Anthony Fu](https://github.com/antfu/skills/tree/d524ed287eef22757297827ce42e3cad88abd20c/skills/vitest) | Тесты frontend, моки, timers, fixtures |
| api-designer | [Jeff Allan](https://github.com/Jeffallan/claude-skills/tree/078a25cc5bfd8011459be7b91d004e900c1d02d3/skills/api-designer) | REST, OpenAPI, ошибки, фильтрация и пагинация |

Точные repo/path/ref сохранены в [codex-skills.json](codex-skills.json).
Установка выполнена системным skill-installer с закреплёнными commit SHA.
Проверены основные инструкции, наличие пакетов и локальные ссылки SKILL.md.
Все четыре пакета содержат документацию/метаданные, без отдельных исполняемых файлов.
Это не гарантия правильности каждого примера: код проверяем тестами и документацией
фактически установленных версий библиотек.

## Уже доступны

- frontend-design — визуальная система и интерфейс.
- web-design-guidelines — проверка UI и доступности.
- playwright и playwright-interactive — браузерные сценарии.
- gh-address-comments — работа с комментариями ревью.
- gh-fix-ci — разбор ошибок GitHub Actions.
- screenshot — снимки для QA.
- openai-docs, skill-installer и skill-creator — системные инструменты Codex.

Эти навыки повторно не устанавливались. Каталог curated помечает openai-docs как
не установленный в пользовательскую папку, но системная копия уже доступна.

## Использование по ролям

- Backend: rust-best-practices + api-designer; проверки проекта — cargo fmt,
  cargo clippy, cargo test и smoke.
- Frontend: frontend-design + vercel-react-best-practices; web-design-guidelines
  при проверке интерфейса.
- QA: vitest для тестов кода, playwright для сценариев браузера.
- GitHub: gh-address-comments и gh-fix-ci по соответствующим задачам.

Можно явно назвать навык в запросе: «Используй rust-best-practices для BE-03».
Новые навыки доступны начиная со следующего сообщения Codex.

## Применение к нашему стеку

- Используем React + Vite: разделы Vercel про Next.js/RSC применимы только при
  наличии соответствующего стека; они не требуют переносить проект на Next.js.
- Rust остаётся на согласованном toolchain и lock-файле. Необязательные рекомендации
  навыка про nightly, cargo update или сторонние Cargo-инструменты не являются
  основанием менять окружение в рамках обычной задачи.
- API Designer помогает описать существующие ограничения локального прототипа;
  шаблоны JWT/GraphQL не добавляются автоматически.
- Vitest skill сгенерирован по 5.1.0-beta.1; проект использует установленную версию
  из package-lock.json. Несовпадающие примеры проверять по её API.
- Встроенные в справочники примеры публикации coverage и внешних интеграций
  не подключаются автоматически.
- Для Axum, SQLx, SQLite и конкретных плат используем исходники проекта и
  документацию их версий; отдельные навыки не являются обязательными.

## Повторение установки для команды

Передать своему Codex файл codex-skills.json и попросить установить перечисленные
навыки через skill-installer, используя указанные ref. Перед установкой проверить
наличие навыка, чтобы не дублировать существующий.

Проверка с этой стороны не устанавливает их в Claude Code: для другого агента
нужна отдельная установка в его поддерживаемый каталог навыков.
