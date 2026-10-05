// WebView2 远程调试口（CDP）的最小封装，供各端到端脚本复用。
//
// 前置：应用以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动。
// 只用 Node 22+ 自带的全局 WebSocket 与 fetch，不装任何依赖。

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/** 连上当前应用页面，返回 { evaluate, pageErrors, close }。 */
export async function connect({ port = 9222, timeoutMs = 30000 } = {}) {
  let page = null
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
      page =
        list.find((t) => t.type === 'page' && t.url.includes('1420')) ||
        list.find((t) => t.type === 'page')
      if (page?.webSocketDebuggerUrl) break
    } catch {
      /* 端口还没起来 */
    }
    await sleep(500)
  }
  if (!page?.webSocketDebuggerUrl) {
    throw new Error(`找不到 WebView2 调试目标（端口 ${port}）`)
  }

  const ws = new WebSocket(page.webSocketDebuggerUrl)
  await new Promise((res, rej) => {
    ws.addEventListener('open', res, { once: true })
    ws.addEventListener('error', rej, { once: true })
  })

  let nextId = 1
  const pageErrors = []
  let onEvent = null

  function send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = nextId++
      const onMsg = (ev) => {
        const m = JSON.parse(ev.data)
        if (m.method === 'Runtime.exceptionThrown') {
          pageErrors.push(
            'exception: ' + (m.params?.exceptionDetails?.exception?.description ?? 'unknown'),
          )
        } else if (m.method === 'Runtime.consoleAPICalled' && m.params?.type === 'error') {
          pageErrors.push('console.error: ' + JSON.stringify(m.params.args?.map((a) => a.value)))
        }
        if (onEvent && m.method) onEvent(m)
        if (m.id === id) {
          ws.removeEventListener('message', onMsg)
          if (m.error) reject(new Error(JSON.stringify(m.error)))
          else resolve(m.result)
        }
      }
      ws.addEventListener('message', onMsg)
      ws.send(JSON.stringify({ id, method, params }))
    })
  }

  await send('Runtime.enable')

  /** 在页面里求值；表达式可以是返回 Promise 的 async IIFE。 */
  async function evaluate(expression) {
    const r = await send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    })
    if (r.exceptionDetails) {
      throw new Error(
        r.exceptionDetails.exception?.description || r.exceptionDetails.text || '页面内求值失败',
      )
    }
    return r.result.value
  }

  return {
    evaluate,
    pageErrors,
    setEventHandler: (fn) => {
      onEvent = fn
    },
    close: () => ws.close(),
  }
}
