# JSON-примеры API v0.3.0

Файлы содержат чистые JSON payload, пригодные для UI fixtures и ручных запросов.
UUID и timestamps в ответах иллюстративные, реальные значения генерирует сервер.
Набор синтетический. Примеры согласованы с [OpenAPI](../openapi.yaml).

| Файл | Использование |
|---|---|
| measurement.request.json | POST /api/measurements с известным временем |
| measurement.equivalent-retry.request.json | Повтор того же сообщения и момента времени с другим offset |
| measurement.unknown-time.request.json | POST без measuredAt |
| measurement.response.json | Ответ 201 или идентичный повтор 200 |
| measurement.unknown-time.response.json | measuredAt: null, timeBasis: receivedAt |
| measurements.response.json | GET /api/measurements |
| health.response.json | GET /api/health |
| assessment.empty.response.json | Пустая БД |
| assessment.recent.response.json | Недавняя запись, правила не подключены |
| assessment.stale.response.json | Устаревшая запись |
| assessment.unknown-time.response.json | Время измерения неизвестно; свежесть не установлена |
| tasks.response.json | GET /api/tasks |
| task-update.request.json | PATCH /api/tasks/daily-check |
| task.response.json | Обновлённая задача |
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
