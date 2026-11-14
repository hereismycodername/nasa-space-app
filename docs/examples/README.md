# JSON-примеры API v0.9.0 (с совместимыми fixtures BE-06)

Файлы содержат чистые JSON payload, пригодные для UI fixtures и ручных запросов.
UUID и timestamps в ответах иллюстративные, реальные значения генерирует сервер.
Набор синтетический. Примеры согласованы с [OpenAPI](../openapi.yaml).
Примеры `assessment.rules-*` получены из настоящего BE-08 API на временной БД:
они включают `health`, версию правил, действия и источники. Их timestamps —
время приёмочного прогона, а не актуальные показания. `assessment.*` без `rules-`
сохранены как fixtures старого BE-06 без `health`, которые клиенты ещё умеют читать.
В `assessment.rules-normal` шесть исходных показателей имеют `noAlert`, новые
шесть — `notEvaluated`; общий статус `insufficientData` явно отражает отсутствие
полного покрытия правилами. Название файла не означает общую норму здоровья.
В старых latest/assessments качество рассчитано на фиксированный `evaluatedAt`
`2026-11-13T08:00:03Z`; это fixtures, а не текущие показания. `quality.measurementId`
связан с последним известным измерением (или последней доставкой, если известного
нет), `latestReceivedQuality.measurementId` — с последней доставкой.
Окна свежести — демонстрационные, не медицинские пороги.

| Файл | Использование |
|---|---|
| measurement.request.json | POST /api/measurements с известным временем |
| measurement.equivalent-retry.request.json | Повтор того же сообщения и момента времени с другим offset |
| measurement.unknown-time.request.json | POST без measuredAt |
| measurement.response.json | Ответ 201 или идентичный повтор 200 |
| measurement.unknown-time.response.json | measuredAt: null, timeBasis: receivedAt |
| measurements.response.json | GET /api/measurements |
| measurements.empty.response.json | Пустая страница истории с hasMore=false |
| measurements.page.response.json | Страница с продолжением hasMore/nextCursor |
| measurements.latest.response.json | Latest с известным измерением и отдельной последней доставкой без времени |
| measurements.latest.empty.response.json | Все шесть метрик без данных, null и quality=missing |
| measurements.latest.quality.response.json | Независимые fresh, stale, missing, unknownTime и clockSkew |
| measurements.series.mean.response.json | Средний пульс, mixed timeBasis, пустой интервал и укороченный последний интервал |
| measurements.series.last.response.json | Последнее значение сна; count и timeBasis учитывают все записи интервала |
| measurements.series.empty.response.json | Пустая сетка mood за 5 ms: maxPoints=2, интервалы 3 ms и 2 ms |
| health.response.json | GET /api/health |
| assessment.empty.response.json | Пустая БД |
| assessment.recent.response.json | Недавняя запись, правила не подключены |
| assessment.stale.response.json | Устаревшая запись |
| assessment.unknown-time.response.json | Время измерения неизвестно; свежесть не установлена |
| assessment.clock-skew.response.json | Время последней записи в будущем |
| assessment.quality.response.json | Свежий пульс и устаревший сон: legacy status не описывает все метрики |
| assessment.rules-normal.response.json | Свежие показатели без срабатывания заданных правил; не заключение о здоровье |
| assessment.rules-attention.response.json | Сон, настроение и пульс требуют внимания; причины, действия и источники |
| assessment.rules-critical.response.json | Высокое систолическое давление; срочный маршрут помощи и evidence ID |
| tasks.response.json | GET /api/tasks?date=2026-11-13: missionDate и список шаблонов с отметками |
| task-update.request.json | PATCH /api/tasks/daily-check с явной missionDate |
| task.response.json | Обновлённая задача с missionDate |
| validation-error.response.json | Problem Details с кодом и полем |

Пример отправки из корня репозитория:

```bash
curl -i http://127.0.0.1:8080/api/measurements \
  -H 'Content-Type: application/json' \
  --data-binary @docs/examples/measurement.request.json
```

Повтор той же команды возвращает исходное измерение. После неё отправка
`measurement.equivalent-retry.request.json` также возвращает 200 и исходную запись:
`13:00:00+05:00` — тот же момент, что `08:00:00Z`, а ответ сохраняет первое написание.
Если начать с примера повтора, он создаст запись с собственным написанием времени.
Изменение value при сохранении
ключа сообщения возвращает 409. Перед другим демонстрационным измерением изменить
sessionId/sequence. Фиксированная дата примера будет устаревшей для оценки свежести;
для живого потока используйте симулятор.

Фильтр истории с URL-кодированием часового пояса:

```bash
curl --get http://127.0.0.1:8080/api/measurements \
  --data-urlencode 'metric=heartRate' \
  --data-urlencode 'from=2026-11-13T13:00:00+05:00' \
  --data-urlencode 'to=2026-11-13T13:01:00+05:00' \
  --data-urlencode 'limit=60'
```

Для следующей страницы передать полученный `nextCursor` через `--data-urlencode`
с теми же фильтрами. Курсор в JSON-примере иллюстративный; в запросах использовать
реальный ответ сервера. `/api/measurements/latest` возвращает все шесть метрик
независимо от этого диапазона и лимита истории.

Ряд пульса за семь дней, максимум 168 точек:

```bash
curl --get http://127.0.0.1:8080/api/measurements/series \
  --data-urlencode 'metric=heartRate' \
  --data-urlencode 'from=2026-11-07T00:00:00Z' \
  --data-urlencode 'to=2026-11-14T00:00:00Z' \
  --data-urlencode 'maxPoints=168'
```

Сервер возвращает 168 часовых интервалов, включая пустые, и учитывает все
подходящие записи, а не первую страницу истории. Для примера mean используется
`from=2026-11-13T08:00:00Z`, `to=2026-11-13T08:00:05Z`, `maxPoints=3`:
первые две записи пульса 70 и 74 дают mean=72, второй интервал пуст,
последний содержит 78. В примере last первые две записи сна могли иметь значения
6 и 7.5: ответ содержит последнее 7.5, count=2, min/max=null.
Это иллюстративные fixtures; команда не создаёт эти измерения в БД.
