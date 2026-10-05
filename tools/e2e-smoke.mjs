// 通过 WebView2 远程调试口驱动图筛界面做端到端冒烟（真点按钮、读真实 DOM 与数据库）。
//
// 用法：
//   1) 造受控图片：python tools/make_test_images.py [目录]
//   2) 带调试口启动应用：
//        $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'
//        pnpm tauri dev
//   3) 跑冒烟：  node tools/e2e-smoke.mjs <图片目录> [隔离区目录]
//
// 依赖 Node 22+ 自带的全局 WebSocket 与 fetch，不需要额外装包。
const IMAGES = process.argv[2]
const QUARANTINE = process.argv[3] || 'D:\\code\\PicSieve\\.e2e\\quarantine'

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

async function findPage() {
  for (let i = 0; i < 60; i++) {
    try {
      const list = await (await fetch('http://127.0.0.1:9222/json/list')).json()
      const page =
        list.find((t) => t.type === 'page' && t.url.includes('1420')) ||
        list.find((t) => t.type === 'page')
      if (page?.webSocketDebuggerUrl) return page
    } catch {
      /* 端口还没起来 */
    }
    await sleep(500)
  }
  throw new Error('找不到 WebView2 调试目标（9222）')
}

const page = await findPage()
const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((res, rej) => {
  ws.addEventListener('open', res, { once: true })
  ws.addEventListener('error', rej, { once: true })
})

let nextId = 1
const consoleErrors = []
function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = nextId++
    const onMsg = (ev) => {
      const m = JSON.parse(ev.data)
      if (m.method === 'Runtime.exceptionThrown') {
        consoleErrors.push(
          'exception: ' + (m.params?.exceptionDetails?.exception?.description ?? 'unknown'),
        )
      } else if (m.method === 'Runtime.consoleAPICalled' && m.params?.type === 'error') {
        consoleErrors.push('console.error: ' + JSON.stringify(m.params.args?.map((a) => a.value)))
      }
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

const flow = `(async () => {
  const IMAGES = ${JSON.stringify(IMAGES)};
  const QUARANTINE = ${JSON.stringify(QUARANTINE)};
  const report = {};
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const buttons = () => Array.from(document.querySelectorAll('button'));
  const byText = (t) => buttons().find((b) => (b.textContent || '').trim().includes(t));

  async function waitFor(fn, what, ms = 30000) {
    const t0 = Date.now();
    let last = '';
    while (Date.now() - t0 < ms) {
      try {
        const v = fn();
        if (v) return v;
        last = 'falsy';
      } catch (e) { last = String(e); }
      await wait(200);
    }
    throw new Error('等待超时: ' + what + ' / ' + last);
  }

  // 1) 直接写设置（原生选目录对话框无法自动化）
  const s = await invoke('get_settings');
  s.roots = [IMAGES];
  s.quarantineDir = QUARANTINE;
  await invoke('save_settings', { settings: s });
  report.settings = { roots: s.roots, quarantineDir: s.quarantineDir, threads: s.threads };

  // 2) 切到「扫描」页
  byText('扫描').click();
  await waitFor(() => byText('开始扫描'), '扫描页出现');
  report.scanTabRendered = true;

  // 3) 点「开始扫描」
  byText('开始扫描').click();
  const summary = await waitFor(() => {
    const p = document.querySelector('.summary');
    return p && p.textContent.includes('新增') ? p.textContent.replace(/\\s+/g, ' ').trim() : null;
  }, '扫描结束统计');
  report.scanSummary = summary;

  // 4) 点「开始计算指纹」
  byText('开始计算指纹').click();
  await waitFor(() => {
    const ps = Array.from(document.querySelectorAll('.summary'));
    return ps.some((p) => p.textContent.includes('指纹计算完成')) ? true : null;
  }, '指纹完成', 60000);
  report.fingerprintDone = true;

  // 5) 切到「图库」页，等缩略图渲染出来
  byText('图库').click();
  await waitFor(() => byText('开始计算指纹') === undefined, '离开扫描页');
  const countText = await waitFor(() => {
    const el = document.querySelector('.count');
    return el && /命中\\s*(\\d+)/.test(el.textContent) && Number(el.textContent.match(/(\\d+)/)[1]) > 0
      ? el.textContent.trim() : null;
  }, '图库命中数 > 0');
  report.libraryCount = countText;

  const thumbCount = await waitFor(
    () => document.querySelectorAll('img[src^="blob:"]').length > 0
      ? document.querySelectorAll('img[src^="blob:"]').length : null,
    '缩略图 blob 出现', 20000);
  report.thumbBlobs = thumbCount;
  report.cells = document.querySelectorAll('.cell').length;
  report.dupBadges = Array.from(document.querySelectorAll('.badge')).map((b) => b.textContent.trim()).filter((t) => t.startsWith('×'));
  report.grayBadges = document.querySelectorAll('.badge.low').length;

  // 6) 勾「只看灰阶」：夹具里只有 1 张灰阶
  const countText2 = () => (document.querySelector('.count')?.textContent || '').trim();
  const baselineCount = report.libraryCount;
  const grayBox = Array.from(document.querySelectorAll('.check input')).find((i) => i.closest('label').textContent.includes('灰阶'));
  grayBox.click();
  report.grayOnlyCount = await waitFor(
    () => (countText2() === '命中 1 张' ? countText2() : null),
    '灰阶筛选后只剩 1 张',
  );
  report.grayOnlyCells = document.querySelectorAll('.cell').length;
  report.grayOnlyBadges = document.querySelectorAll('.badge.low').length;
  grayBox.click();
  await waitFor(() => (countText2() === baselineCount ? true : null), '取消灰阶筛选回到全部');

  // 7) 勾「有重复的」：夹具里那张一模一样的图存了两份
  const dupBox = Array.from(document.querySelectorAll('.check input')).find((i) => i.closest('label').textContent.includes('有重复的'));
  dupBox.click();
  report.dupOnlyCount = await waitFor(
    () => (countText2() === '命中 2 张' ? countText2() : null),
    '重复筛选后剩 2 张',
  );
  report.dupOnlyCells = document.querySelectorAll('.cell').length;
  dupBox.click();
  await waitFor(() => (countText2() === baselineCount ? true : null), '取消重复筛选回到全部');

  // 7b) 勾「读不出的」：夹具里有一个坏文件
  const errBox = Array.from(document.querySelectorAll('.check input')).find((i) => i.closest('label').textContent.includes('读不出的'));
  errBox.click();
  report.decodeErrorOnlyCount = await waitFor(
    () => (countText2() === '命中 1 张' ? countText2() : null),
    '读不出的筛选后剩 1 张',
  );
  errBox.click();
  await waitFor(() => (countText2() === baselineCount ? true : null), '取消读不出的筛选回到全部');

  // 8) 选中第一张，底部计数应变化
  await waitFor(() => document.querySelector('.cell'), '有格子');
  document.querySelector('.cell').click();
  report.actionBar = await waitFor(() => {
    const el = document.querySelector('.bar');
    return el && el.textContent.includes('1') ? el.textContent.replace(/\\s+/g, ' ').trim() : null;
  }, '底部已选 1 张');

  // 8b) 重复组视图。
  // 夹具里缩略版与原图的 pHash 距离为 9，默认阈值 8 抓不到，这里把阈值放宽到 12，
  // 顺便验证「相似阈值可在设置里调」这条链路。
  const st = await invoke('get_settings');
  st.similarThreshold = 12;
  await invoke('save_settings', { settings: st });

  byText('重复组').click();
  await waitFor(() => byText('重建分组'), '重复组页出现');
  byText('重建分组').click();
  report.rebuildMsg = await waitFor(() => {
    const el = document.querySelector('.msg');
    return el && el.textContent.includes('一模一样') ? el.textContent.trim() : null;
  }, '重建分组完成', 60000);

  report.groupTitle = await waitFor(() => {
    const h = document.querySelector('.head strong');
    return h && h.textContent.includes('组') ? h.textContent.replace(/\\s+/g, ' ').trim() : null;
  }, '分组标题');
  report.groupCards = document.querySelectorAll('.card').length;
  report.keepCards = document.querySelectorAll('.card.keep').length;
  report.groupThumbs = document.querySelectorAll('.card img[src^="blob:"]').length;
  report.groupFooter = (document.querySelector('.foot')?.textContent || '').replace(/\\s+/g, ' ').trim();
  const keepPath = () => document.querySelector('.card.keep code')?.textContent || '';
  report.groupKeepPath = keepPath();

  // 「改留这张」应把蓝框换到另一张
  const swapBtn = Array.from(document.querySelectorAll('.card button')).find((b) => b.textContent.includes('改留这张'));
  if (swapBtn) {
    report.swapAvailable = true;
    swapBtn.click();
    await waitFor(() => {
      const p = keepPath();
      return p && p !== report.groupKeepPath ? p : null;
    }, '保留项已换人');
    report.groupKeepPathAfterSwap = keepPath();
    report.keepChanged = report.groupKeepPathAfterSwap !== report.groupKeepPath;
  }

  // 切到「看着像」：应只看到 1 组（大图 + 缩略版），完全相同的副本不再重复出现
  const kindSel = document.querySelector('.head select');
  kindSel.value = 'similar';
  kindSel.dispatchEvent(new Event('change'));
  await wait(800);
  report.similarTitle = (document.querySelector('.head strong')?.textContent || '').replace(/\\s+/g, ' ').trim();
  report.similarCards = document.querySelectorAll('.card').length;

  // 9) 后端数据侧对照：直接查库
  const filter = { minShortSide: null, maxShortSide: null, minSize: null, maxSize: null,
    exts: [], onlyGray: false, onlyDuplicated: false, onlyDecodeError: false,
    search: null, sort: 'sizeDesc', limit: 100, offset: 0 };
  const all = await invoke('query_files_cmd', { filter });
  const errs = await invoke('query_files_cmd', { filter: { ...filter, onlyDecodeError: true } });
  const grays = await invoke('query_files_cmd', { filter: { ...filter, onlyGray: true } });
  report.dbTotal = all.length;
  report.dbDecodeErrors = errs.length;
  report.dbGray = grays.length;
  report.dbFields = all.map((f) => ({
    path: f.path.split('\\\\').pop(), w: f.width, h: f.height, short: f.shortSide,
    hash: f.contentHash ? f.contentHash.slice(0, 8) : null,
    phash: f.phash, gray: f.grayScore, pid: f.pid,
  }));

  return report;
})()`

const result = await send('Runtime.evaluate', {
  expression: flow,
  awaitPromise: true,
  returnByValue: true,
})

if (result.exceptionDetails) {
  console.log(JSON.stringify({ ok: false, error: result.exceptionDetails, pageErrors: consoleErrors }, null, 2))
  process.exit(1)
}
console.log(JSON.stringify({ ok: true, report: result.result.value, pageErrors: consoleErrors }, null, 2))
ws.close()
