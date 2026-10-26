// The smallest window: `bun examples/counter.tsx` (after `cargo build -p pumi-gpuix`).
import { useState } from "react"
import { render } from "pumi"

function Counter() {
  const [count, setCount] = useState(0)
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12, padding: 24, height: "100%", backgroundColor: "#1a1a1a" }}>
      <text style={{ color: "#e2e2e2", fontSize: 18 }}>{`${count} clicks`}</text>
      <div
        onClick={() => setCount((c) => c + 1)}
        style={{ padding: 12, borderRadius: 8, cursor: "pointer", backgroundColor: "#232323", hover: { backgroundColor: "#2c2c2c" } }}
      >
        <text style={{ color: "#e2e2e2" }}>+1</text>
      </div>
    </div>
  )
}

render(<Counter />, { title: "pumi", width: 480, height: 320 })
