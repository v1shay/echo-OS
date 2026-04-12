import { Routes, Route } from 'react-router-dom'
import { RootRedirect } from './components/RootRedirect'
import { OnboardingPage } from './pages/OnboardingPage'
import { HomePage } from './pages/HomePage'
import { WorkoutPage } from './pages/WorkoutPage'
import { SessionSummaryPage } from './pages/SessionSummaryPage'
import { RecoveryLogPage } from './pages/RecoveryLogPage'
import { ProfilePage } from './pages/ProfilePage'
import { FriendsPage } from './pages/FriendsPage'

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<RootRedirect />} />
      <Route path="/onboarding" element={<OnboardingPage />} />
      <Route path="/home" element={<HomePage />} />
      <Route path="/workout" element={<WorkoutPage />} />
      <Route path="/session-summary" element={<SessionSummaryPage />} />
      <Route path="/recovery-log" element={<RecoveryLogPage />} />
      <Route path="/profile" element={<ProfilePage />} />
      <Route path="/friends" element={<FriendsPage />} />
    </Routes>
  )
}
