import { createFileRoute } from '@tanstack/react-router'
import { TrafficPage } from '@/pages/traffic'

export const Route = createFileRoute('/_app/traffic')({
  component: TrafficPage,
})
