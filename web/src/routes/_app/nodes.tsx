import { createFileRoute } from '@tanstack/react-router'
import { NodesPage } from '@/pages/nodes'

export const Route = createFileRoute('/_app/nodes')({
  component: NodesPage,
})
