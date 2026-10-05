// 隔离区破坏性端到端验证：真点界面「移入隔离区 → 搬回来 → 彻底清空」，
// 每一步都在 Node 侧核对磁盘上的真实文件与内容指纹。
//
// 用法（需应用已带 --remote-debugging-port=9222 启动）：
//   node tools/e2e-quarantine.mjs
//
// 计划里写明「这一步不许跳过」：它是唯一能证明「移入的文件能原样搬回」的验证。
// 断言全部按增量写，库里已有的其他记录不影响结果。

import { createHash } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { connect } from './cdp.mjs'

const ROOT = path.resolve('.e2e/destructive')
const IMAGES = path.join(ROOT, 'images')
const QDIR = path.join(ROOT, 'quarantine')

// 1x1 PNG：自带夹具，不依赖外部素材，也不读用户真实图片
const PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  'base64',
)

const sha = (p) => createHash('sha256').update(fs.readFileSync(p)).digest('hex')
const listQ = () => (fs.existsSync(QDIR) ? fs.readdirSync(QDIR) : [])

const checks = []
function check(name, ok, detail = '') {
  checks.push({ name, ok, detail })
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '  → ' + detail : ''}`)
}

/** 等磁盘状态变化（界面提示只说明命令发出去了，落盘才算数）。 */
async function waitUntil(pred, what, ms = 20000) {
  const t0 = Date.now()
  while (Date.now() - t0 < ms) {
    try {
      if (pred()) return
    } catch {
      /* 继续等 */
    }
    await new Promise((r) => setTimeout(r, 200))
  }
  throw new Error('等待超时: ' + what)
}

// 清空夹具目录，放一张内容已知的图
fs.rmSync(ROOT, { recursive: true, force: true })
fs.mkdirSync(IMAGES, { recursive: true })
fs.mkdirSync(QDIR, { recursive: true })
const victim = path.join(IMAGES, 'only.png')
fs.writeFileSync(victim, PNG)
const before = sha(victim)

const { evaluate, pageErrors, close } = await connect()

const PRELUDE = `
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const wait = async (fn, what, ms = 30000) => {
    const t0 = Date.now();
    let last = '';
    while (Date.now() - t0 < ms) {
      try { const v = fn(); if (v) return v; last = 'falsy'; }
      catch (e) { last = String(e); }
      await sleep(200);
    }
    throw new Error('等待超时: ' + what + ' / ' + last);
  };
  const byText = (t) => Array.from(document.querySelectorAll('button'))
    .find((b) => (b.textContent || '').trim().includes(t));
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
  const VICTIM = ${JSON.stringify(victim)};
  const ALL = { minShortSide: null, maxShortSide: null, minSize: null, maxSize: null,
    exts: [], onlyGray: false, onlyDuplicated: false, onlyDecodeError: false,
    search: null, sort: 'sizeDesc', limit: 300, offset: 0 };
  const totalCount = () => invoke('count_files_cmd', { filter: ALL });
  // 图库网格的顺序与这个查询一致，因此能按下标点到目标那张
  const cellIndexOfVictim = async () => {
    const list = await invoke('query_files_cmd', { filter: ALL });
    return list.findIndex((f) => f.path === VICTIM);
  };
  const clickVictimCell = async () => {
    const i = await cellIndexOfVictim();
    if (i < 0) throw new Error('图库里找不到夹具文件');
    const cells = document.querySelectorAll('.cell');
    if (!cells[i]) throw new Error('格子还没渲染出来: ' + i + '/' + cells.length);
    cells[i].click();
    await wait(() => (document.querySelector('.bar')?.textContent || '').includes('1 张'), '选中 1 张');
  };
  // 提示条会停留 5 秒，第二次操作前必须等它消失，否则会读到上一条旧提示
  const moveVictim = async () => {
    await wait(() => !document.querySelector('.notice'), '上一条提示消失');
    await clickVictimCell();
    byText('移到隔离区').click();
    return await wait(() => {
      const n = document.querySelector('.notice');
      return n && n.textContent.includes('已移入隔离区') ? n.textContent.trim() : null;
    }, '移入提示');
  };
`
const run = (body) => evaluate(`(async () => {${PRELUDE}\n${body}\n})()`)

// 0) 先记下当前库里的总数，后面全部按增量断言
const countBefore = await run('return await totalCount();')

// 1) 写设置 + 扫描 + 进图库
const setup = await run(`
  const s = await invoke('get_settings');
  s.roots = [${JSON.stringify(IMAGES)}];
  s.quarantineDir = ${JSON.stringify(QDIR)};
  await invoke('save_settings', { settings: s });

  byText('扫描').click();
  await wait(() => byText('开始扫描'), '扫描页');
  byText('开始扫描').click();
  const summary = await wait(() => {
    const p = document.querySelector('.summary');
    return p && p.textContent.includes('新增') ? p.textContent.replace(/\\s+/g, ' ').trim() : null;
  }, '扫描结束');

  byText('图库').click();
  await wait(() => document.querySelectorAll('.cell').length > 0, '格子渲染');
  const after = await totalCount();
  const victimIndex = await cellIndexOfVictim();
  return { summary, after, victimIndex };
`)
check(
  '扫描把夹具入库（库里能找到它）',
  setup.victimIndex >= 0,
  `before=${countBefore} after=${setup.after} ${setup.summary}`,
)

// 2) 选中目标文件并移入隔离区
const moved = await run('return await moveVictim();')
check('界面反馈已移入', String(moved).includes('已移入隔离区 1 张'), moved)

// 3) 磁盘核对：原位置没了、隔离区里内容一字不差
await waitUntil(() => !fs.existsSync(victim), '源文件被移走')
check('原位置已清空', !fs.existsSync(victim))
const q1 = listQ()
check('隔离区里有 1 个文件', q1.length === 1, q1.join(', '))
const quarantined = q1.length ? path.join(QDIR, q1[0]) : null
check('隔离区文件内容与原件一致', !!quarantined && sha(quarantined) === before)

// 4) 隔离区页应看到这批，点「搬回来」
const qView = await run(`
  byText('隔离区').click();
  return await wait(() => {
    const tr = document.querySelector('tbody tr');
    if (!tr || tr.textContent.includes('隔离区是空的')) return null;
    const tds = Array.from(tr.children).map((td) => td.textContent.replace(/\\s+/g, ' ').trim());
    return tds;
  }, '隔离区批次出现');
`)
check('隔离区列出该批次（1 张、有搬回来）', qView[1] === '1' && qView.join(' ').includes('搬回来'), JSON.stringify(qView))

const restored = await run(`
  byText('搬回来').click();
  return await wait(() => {
    const m = document.querySelector('.msg');
    return m && m.textContent.includes('已搬回') ? m.textContent.trim() : null;
  }, '搬回完成');
`)
await waitUntil(() => fs.existsSync(victim), '文件被搬回')
check('界面反馈已搬回', String(restored).includes('已搬回 1 张'), restored)

// 5) 磁盘核对：回到原位、内容不变
check('文件回到原位', fs.existsSync(victim))
check('搬回后内容逐字节不变', fs.existsSync(victim) && sha(victim) === before)
check('隔离区已清空该批', listQ().length === 0, listQ().join(', '))

// 6) 再移入一次，然后走「彻底清空」的二次确认
await run(`
  byText('图库').click();
  await wait(() => document.querySelectorAll('.cell').length > 0, '回到图库');
  return await moveVictim();
`)
await waitUntil(() => !fs.existsSync(victim) && listQ().length === 1, '第二次移入落盘')
check('第二次移入后原位置再次清空', !fs.existsSync(victim))
check('第二次移入后隔离区有 1 个文件', listQ().length === 1, listQ().join(', '))

const confirmText = await run(`
  byText('隔离区').click();
  await wait(() => document.querySelector('tbody tr'), '隔离区页面');
  byText('彻底清空').click();
  return await wait(() => {
    const w = document.querySelector('.warn');
    return w ? w.textContent.replace(/\\s+/g, ' ').trim() : null;
  }, '二次确认文案');
`)
check('清空前展示张数与释放空间', /\b1\b/.test(confirmText) && /MB|GB/.test(confirmText), confirmText)

const purged = await run(`
  byText('确定永久删除').click();
  return await wait(() => {
    const m = document.querySelector('.msg');
    return m && m.textContent.includes('已永久删除') ? m.textContent.trim() : null;
  }, '清空完成');
`)
check('界面反馈已永久删除', String(purged).includes('已永久删除 1 张'), purged)

// 7) 磁盘核对：隔离区与源目录都不该再有这个文件
check('隔离区文件已真的删除', listQ().length === 0, listQ().join(', '))
check('源目录也没有复活', !fs.existsSync(victim))
check('页面无 JS 报错', pageErrors.length === 0, pageErrors.join(' | '))

close()

const failed = checks.filter((c) => !c.ok)
console.log(`\n${checks.length - failed.length}/${checks.length} 项通过`)
if (failed.length) {
  console.log(JSON.stringify({ ok: false, failed }, null, 2))
  process.exit(1)
}
console.log(JSON.stringify({ ok: true }, null, 2))
