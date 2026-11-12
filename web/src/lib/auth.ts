export type Admin = {
  id: string
  email: string
  role: 'admin' | 'user'
  createdAt: number
}

const TOKEN_KEY = 'rv_token'
const ADMIN_KEY = 'rv_admin'

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}

export function getAdmin(): Admin | null {
  const raw = localStorage.getItem(ADMIN_KEY)
  if (!raw) return null
  try {
    return JSON.parse(raw) as Admin
  } catch {
    return null
  }
}

export function setAuth(token: string, admin: Admin) {
  localStorage.setItem(TOKEN_KEY, token)
  localStorage.setItem(ADMIN_KEY, JSON.stringify(admin))
}

export function clearAuth() {
  localStorage.removeItem(TOKEN_KEY)
  localStorage.removeItem(ADMIN_KEY)
}
