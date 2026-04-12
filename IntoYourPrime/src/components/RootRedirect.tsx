import { useEffect, useState } from 'react'
import { Navigate } from 'react-router-dom'
import { onAuthStateChanged, type User } from 'firebase/auth'
import { auth } from '../lib/firebase'

export function RootRedirect() {
  const [ready, setReady] = useState(false)
  const [user, setUser] = useState<User | null>(null)

  useEffect(() => {
    const unsub = onAuthStateChanged(auth, (u) => {
      setUser(u)
      setReady(true)
    })
    return unsub
  }, [])

  if (!ready) {
    return (
      <div className="flex min-h-screen items-center justify-center text-slate-500">
        Loading…
      </div>
    )
  }

  return <Navigate to={user ? '/home' : '/onboarding'} replace />
}
