/**
 * 登录页交互测试。
 *
 * 验证提交流程（调用 RPC、保存会话、跳转首页）与失败分支（停留在登录页
 * 并展示错误提示）。路由、RPC 与 toast 均被 mock，不依赖主节点。
 */
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { toast } from 'sonner'
import { LoginPage } from './login'

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  login: vi.fn(),
  setAuth: vi.fn(),
  error: vi.fn(),
}))

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mocks.navigate,
}))

vi.mock('@/lib/rpc', () => ({
  login: mocks.login,
}))

vi.mock('@/lib/auth', () => ({
  setAuth: mocks.setAuth,
}))

vi.mock('sonner', () => ({
  toast: { error: mocks.error },
}))

const admin = { id: 'admin-1', email: 'admin@example.com', role: 'admin' as const, createdAt: 1 }

describe('LoginPage', () => {
  beforeEach(() => {
    mocks.navigate.mockReset()
    mocks.login.mockReset()
    mocks.setAuth.mockReset()
    mocks.error.mockReset()
  })

  it('submits credentials and stores the session on success', async () => {
    mocks.login.mockResolvedValue({ token: 'session-token', admin })
    const user = userEvent.setup()

    render(<LoginPage />)
    await user.clear(screen.getByLabelText('邮箱'))
    await user.type(screen.getByLabelText('邮箱'), 'ops@example.com')
    await user.type(screen.getByLabelText('密码'), 'password123')
    await user.click(screen.getByRole('button', { name: '登录' }))

    await waitFor(() => {
      expect(mocks.login).toHaveBeenCalledWith('ops@example.com', 'password123')
    })
    expect(mocks.setAuth).toHaveBeenCalledWith('session-token', admin)
    expect(mocks.navigate).toHaveBeenCalledWith({ to: '/' })
  })

  it('stays on the page and shows an error when login fails', async () => {
    mocks.login.mockRejectedValue(new Error('密码错误'))
    const user = userEvent.setup()

    render(<LoginPage />)
    await user.type(screen.getByLabelText('密码'), 'wrong-password')
    await user.click(screen.getByRole('button', { name: '登录' }))

    await waitFor(() => {
      expect(mocks.login).toHaveBeenCalled()
    })
    expect(toast.error).toHaveBeenCalledWith('密码错误')
    expect(mocks.setAuth).not.toHaveBeenCalled()
    expect(mocks.navigate).not.toHaveBeenCalled()
  })
})
