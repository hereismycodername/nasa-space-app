import type { SystemId } from './types'

export const anatomySystems: { system: SystemId; label: string; fullLabel: string }[] = [
  { system: 'brain', label: 'Мозг', fullLabel: 'Сон и самочувствие' },
  { system: 'eyes', label: 'Зрение', fullLabel: 'Зрение' },
  { system: 'lungs', label: 'Дыхание', fullLabel: 'Дыхание' },
  { system: 'heart', label: 'Сердце', fullLabel: 'Сердце и кровообращение' },
  { system: 'gut', label: 'Обмен', fullLabel: 'Обмен веществ' },
  { system: 'msk', label: 'Кости', fullLabel: 'Кости и мышцы' },
  { system: 'immune', label: 'Всё тело', fullLabel: 'Температура тела' },
]

export type AnatomyMode = 'organs' | 'skeleton' | 'xray'
