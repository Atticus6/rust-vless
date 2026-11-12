import { createFileRoute } from '@tanstack/react-router'
import { VersionsPage } from '@/pages/versions'

export const Route = createFileRoute('/_app/versions')({
  component: VersionsPage,
})
