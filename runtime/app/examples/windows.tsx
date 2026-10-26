// Two windows in one app: `bun examples/windows.tsx`.
import { useState } from "react"
import { createRenderer, createRoot, startFrameLoop } from "pumi"

function Counter({ name }: { name: string }) {
  const [count, setCount] = useState(0)
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12, padding: 24, height: "100%", backgroundColor: "#1a1a1a" }}>
      <text style={{ color: "#e2e2e2", fontSize: 18 }}>{`${name}: ${count} clicks`}</text>
      <div onClick={() => setCount((c) => c + 1)} style={{ padding: 12, borderRadius: 8, backgroundColor: "#232323", hover: { backgroundColor: "#2c2c2c" } }}>
        <text style={{ color: "#e2e2e2" }}>+1</text>
      </div>
    </div>
  )
}

for (const name of ["one", "two"]) {
  const renderer = createRenderer()
  renderer.init({ title: name, width: 420, height: 260 })
  createRoot(renderer).render(<Counter name={name} />)
  if (name === "one") startFrameLoop(renderer)
}
