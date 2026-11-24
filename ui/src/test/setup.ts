// Resets the fake Tauri bridge after each test file.
import { clearMocks } from '@tauri-apps/api/mocks'
import { afterAll } from 'vitest'

afterAll(() => clearMocks())
