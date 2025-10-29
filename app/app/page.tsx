"use client"

import { useState, useEffect } from "react"
import { Moon, Sun } from "lucide-react"

export default function MCPNexus() {
  const [mounted, setMounted] = useState(false)
  const [darkMode, setDarkMode] = useState(true)

  useEffect(() => {
    setMounted(true)
  }, [])

  if (!mounted) {
    return null
  }

  return (
    <div className={darkMode ? "dark" : ""}>
      <div className="min-h-screen bg-[#0a0a0a] text-foreground relative">
        <div
          className="absolute inset-0 opacity-20 will-change-transform"
          style={{
            backgroundImage:
              "url('https://cdn.dribbble.com/userupload/31751763/file/original-e3ba1e6160b8fd1de7b27d9a98176002.gif')",
            backgroundSize: "cover",
            backgroundPosition: "center",
            backgroundRepeat: "no-repeat",
          }}
        />

        {/* Hero Section */}
        <section className="relative min-h-screen flex flex-col items-center justify-center px-4">
          <div className="absolute inset-0 opacity-10 z-[1] pointer-events-none">
            <div
              className="absolute inset-0"
              style={{
                backgroundImage: "radial-gradient(circle, #404040 1px, transparent 1px)",
                backgroundSize: "40px 40px",
              }}
            />
          </div>

          {/* Hero content */}
          <div className="relative z-10 max-w-4xl w-full space-y-12 text-center">
            <div className="space-y-4">
              <h1 className="text-6xl md:text-8xl font-bold text-white tracking-tight">Librarian</h1>
              <p className="text-2xl md:text-4xl text-zinc-400 font-normal tracking-wide"> MCP library where agents query and plug into endpoints autonomously via x402 micropayments</p>
            </div>

            <div className="max-w-2xl mx-auto">
              <p className="text-sm text-zinc-500 mb-4">Access the library:</p>
              <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-6 text-left">
                <code className="text-green-400 text-sm md:text-base break-all">
                  curl -X GET https://api.mcpnexus.ai/library
                </code>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  )
}
