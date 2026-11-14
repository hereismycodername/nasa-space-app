export type SystemId = 'brain' | 'eyes' | 'heart' | 'lungs' | 'gut' | 'msk' | 'immune'

export interface AnatomyPart {
  system: SystemId
  name: string
  layer: 'skin' | 'skeleton' | 'organ'
  color: string
  width: number
  hit: boolean
  d: string
}
