# BE-08: объяснимая оценка показателей

Версия правил `be08-demo-1`. Это проверяемый прототип поддержки самонаблюдения,
не валидированная система диагностики и не медицинский протокол NASA.
Земные рекомендации используются как справочные ориентиры; индивидуальный план
экипажа и указания полётного врача имеют приоритет. Причину изменения показателя
по этим данным установить нельзя. Лекарства и дозировки приложение не назначает.

## Совместимый HTTP-контракт

`GET /api/assessments` сохраняет все поля BE-06 и добавляет `health`:

```ts
type RuleStatus = 'notEvaluated' | 'insufficientData' | 'noAlert' | 'attention' | 'critical'
interface HealthEvaluation {
  ruleSetVersion: string // be08-demo-1
  demonstrationOnly: true
  status: RuleStatus
  explanation: string
  disclaimer: string
  emergencyGuidance: string
  evaluatedMetricCount: number
  totalMetricCount: number // число зарегистрированных показателей
  rules: HealthRule[] // одна запись на каждый показатель реестра
  education: FlightEducation[] // пять опасностей космического полёта
}
interface HealthRule {
  ruleId: string // be08-heartRate и аналогично для остальных metric
  ruleVersion: string // be08-demo-1
  metric: Metric
  title: string
  status: RuleStatus
  explanation: string // значение, единица, критерий и ограничение интерпретации
  measurementIds: string[] // ровно один ID для оценённого результата, иначе []
  validUntil: string | null // измерение + окно BE-06; RFC3339, граница включена
  suggestedAction: string
  escalation: 'none' | 'self' | 'crew' | 'ground' | 'urgent'
  flightContext: string
  sourceUrls: string[]
  demonstrationOnly: true
}
interface FlightEducation {
  id: string // gravity, isolation, radiation, distance, environment
  title: string
  explanation: string
  suggestedAction: string
  sourceUrls: string[]
}
```

Время вычисления — существующее верхнеуровневое `evaluatedAt`. Оценка использует
тот же snapshot и то же качество, что `metrics` этого ответа. Подходит только
`latestKnownMeasurement`, у которого quality=fresh и совпадает measurementId.
Missing, stale, unknownTime, clockSkew дают insufficientData, validUntil=null,
пустые measurementIds и предложение получить корректное измерение.
Свежая доставка старого/неизвестного измерения не делает его пригодным.

Исключение имеет приоритет: если показатель зарегистрирован, но правила для него
ещё нет (новые метрики B/C), результат всегда `notEvaluated`, независимо от
свежести. Он не входит в `evaluatedMetricCount`, блокирует общий `noAlert` и
возвращает `measurementIds=[]`, `validUntil=null`, `sourceUrls=[]`. Наличие записи
не позволяет интерфейсу самостоятельно присвоить такому показателю норму.

Общий статус: critical при любом critical; иначе attention при любом attention;
иначе noAlert только при пригодных результатах для всех показателей, иначе insufficientData.
`noAlert` означает отсутствие срабатываний заданных правил, а не здоровье всех
органов. Пульс и давление оцениваются отдельно, без вывода о причине или диагнозе.
Отсутствие второй части давления не скрывает критический сигнал первой части.

Клиенты используют время с момента получения ответа, validUntil и совпадение
ID с текущим latest. Offline, истёкшая оценка или изменившийся ID не сохраняют
зелёный статус. Предыдущее предупреждение можно читать с явной отметкой времени,
но оно не выдаётся за текущее. Числовые правила выполняются только на сервере.

## Правила и следующий шаг

| Показатель | attention | critical | Ограничения и действие |
|---|---|---|---|
| heartRate, bpm | <60 или >100 | нет числового порога | Ориентир только для покоя; нагрузка и тренированность меняют пульс. Перепроверить в покое, при повторении связаться с медиком; при выраженных симптомах — немедленно экипаж |
| systolicPressure, mmHg | <90 или >=120 | >180 | 120–129: повтор и привычки по плану; >=130 или <90: обсудить с медиком. При >180 немедленно сообщить ответственному за медпомощь; без тревожных симптомов повторить не раньше чем через минуту, при сохранении — срочная консультация |
| diastolicPressure, mmHg | <60 или >=80 | >120 | Перепроверить обе части давления; при >120 тот же порядок срочной помощи |
| temperature, °C | >=38 | <35 | При повышении повторить по инструкции прибора, сообщить медработнику при сохранении/ухудшении; при <35 немедленно привлечь экипаж и медицинскую помощь |
| sleepHours, h | <7 | нет | Запланировать защищённое время сна, обсудить нагрузку с экипажем; при повторении обратиться за поддержкой |
| mood, 1–5 | <3 | нет | Условный порог команды для предложения поддержки, не диагностическая шкала. Предложить приватный разговор, отдых и помощь специалиста при сохранении трудностей |

При боли в груди, выраженной одышке, потере сознания, внезапной слабости либо
нарушении речи/зрения следует немедленно привлечь экипаж и действовать по
аварийному медицинскому протоколу миссии; не ждать обновления приложения или
ответа с Земли. Для срочного высокого давления этот порядок показывается
до предложения повторить измерение. Приложение само никому сообщения не отправляет.

## Космический контекст

- Гравитация: перестройка кровообращения, снижение нагрузки на кости и мышцы.
  Следовать назначенному плану упражнений; шесть показателей не измеряют потерю
  костной ткани и не заменяют профильные исследования.
- Изоляция: стресс, сон и циркадный ритм могут влиять на самочувствие и работоспособность.
  Планировать отдых и общение; оценка настроения не устанавливает психиатрический диагноз.
- Радиация: долгосрочные риски требуют дозиметрии и процедур защиты миссии.
  Пульс, температура и настроение не измеряют дозу и не диагностируют лучевое поражение.
- Удалённость от Земли: помощь и связь могут задерживаться; при срочном состоянии
  привлекают экипаж и используют утверждённые бортовые процедуры.
- Замкнутая среда: важны воздух, шум и контроль микроорганизмов; соблюдать процедуры
  среды/гигиены. По одной температуре нельзя определить инфекцию или причину изменения.

## Источники

Проверены 15 ноября 2026 года. Это ссылки на основания для обучения и осторожных
подсказок, а не заявление об одобрении продукта этими организациями.

- [NASA: пять опасностей](https://www.nasa.gov/hrp/hazards/).
- [NASA: риски для здоровья](https://www.nasa.gov/hrp/risks/).
- [NASA: тело в космосе](https://www.nasa.gov/humans-in-space/the-human-body-in-space/).
- [NASA: изоляция](https://www.nasa.gov/hrp/hazard-isolation-and-confinement/).
- [AHA: пульс](https://www.heart.org/en/health-topics/high-blood-pressure/the-facts-about-high-blood-pressure/all-about-heart-rate-pulse).
- [AHA: категории давления](https://www.heart.org/en/health-topics/high-blood-pressure/understanding-blood-pressure-readings).
- [AHA: очень высокое давление](https://www.heart.org/en/health-topics/high-blood-pressure/understanding-blood-pressure-readings/when-to-call-911-for-high-blood-pressure).
- [AHA: низкое давление](https://www.heart.org/en/health-topics/high-blood-pressure/the-facts-about-high-blood-pressure/low-blood-pressure-when-blood-pressure-is-too-low).
- [AHA: измерение давления](https://www.heart.org/en/health-topics/high-blood-pressure/understanding-blood-pressure-readings/monitoring-your-blood-pressure-at-home).
- [NHS: повышенная температура](https://www.nhs.uk/symptoms/fever-in-adults/).
- [NHS: гипотермия](https://www.nhs.uk/conditions/hypothermia/).
- [CDC: сон](https://www.cdc.gov/sleep/about/).

Порог mood<3 — решение для демонстрационного UX; NASA не задаёт этот порог.
