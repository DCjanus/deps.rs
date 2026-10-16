import { makeBadge } from 'badge-maker'

const cases = await Bun.stdin.json()
process.stdout.write(JSON.stringify(cases.map(testCase => makeBadge(testCase))))
