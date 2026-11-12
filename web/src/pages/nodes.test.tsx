/**
 * 节点页交互测试。
 *
 * 覆盖一次性 token 的展示与清理：创建成功后启动命令必须包含 token，关闭
 * 弹窗后该状态必须消失，避免 token 长期留在页面上。
 */
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { NodesPage } from './nodes'

const mocks = vi.hoisted(() => ({
  listNodes: vi.fn(),
  createNode: vi.fn(),
  updateNode: vi.fn(),
  deleteNode: vi.fn(),
}))

vi.mock('@/lib/rpc', () => ({
  listNodes: mocks.listNodes,
  createNode: mocks.createNode,
  updateNode: mocks.updateNode,
  deleteNode: mocks.deleteNode,
}))

const master = {
  id: 'master-1',
  name: 'master',
  status: 'online',
  version: '0.1.0',
  httpUrl: '',
  rpcUrl: '',
  isMaster: true,
  lastSeen: 1,
  createdAt: 1,
  activeConnections: 0,
  uptimeSec: 0,
}

describe('NodesPage', () => {
  beforeEach(() => {
    mocks.listNodes.mockReset().mockResolvedValue([master])
    mocks.createNode.mockReset()
    mocks.updateNode.mockReset()
    mocks.deleteNode.mockReset()
  })

  it('shows the one-time token in the start command and clears it on close', async () => {
    const tokyo = { ...master, id: 'edge-1', name: 'tokyo', isMaster: false }
    // 第一次加载只有 master；创建成功后的第二次加载必须包含新节点，才能
    // 证明页面确实刷新了列表而不只是弹窗状态变化。
    mocks.listNodes
      .mockResolvedValueOnce([master])
      .mockResolvedValueOnce([master, tokyo])
    mocks.createNode.mockResolvedValue({
      node: tokyo,
      token: 'one-time-token',
    })
    const user = userEvent.setup()

    render(<NodesPage />)
    await screen.findByText('master')

    await user.click(screen.getByRole('button', { name: /创建节点/ }))
    const nameField = await screen.findByLabelText('名称')
    await user.type(nameField, 'tokyo')
    await user.click(screen.getByRole('button', { name: '创建' }))

    await waitFor(() => {
      expect(mocks.createNode).toHaveBeenCalledWith('tokyo')
    })
    expect(await screen.findByText(/one-time-token/)).toBeInTheDocument()
    expect(screen.getByText('节点已创建')).toBeInTheDocument()
    expect(mocks.listNodes).toHaveBeenCalledTimes(2)
    expect(await screen.findByText('tokyo')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: '完成' }))
    await waitFor(() => {
      expect(screen.queryByText(/one-time-token/)).not.toBeInTheDocument()
    })
    expect(screen.queryByText('节点已创建')).not.toBeInTheDocument()
  })

  it('does not call the RPC when the name is blank', async () => {
    const user = userEvent.setup()

    render(<NodesPage />)
    await screen.findByText('master')
    await user.click(screen.getByRole('button', { name: /创建节点/ }))

    const create = await screen.findByRole('button', { name: '创建' })
    expect(create).toBeDisabled()
    expect(mocks.createNode).not.toHaveBeenCalled()
  })
})
