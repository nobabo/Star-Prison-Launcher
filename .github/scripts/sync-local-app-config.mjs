import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const projectRoot = resolve(scriptDirectory, '..', '..')
const sourcePath = join(projectRoot, 'config', 'app.config.json')
const storageRoot = process.env.STAR_PRISON_STORAGE_ROOT
    || (process.env.APPDATA ? join(process.env.APPDATA, 'star-prison-launcher') : '')

if(storageRoot.length === 0){
    throw new Error('APPDATA 또는 STAR_PRISON_STORAGE_ROOT가 필요합니다.')
}

const targetPath = join(storageRoot, 'config', 'app.config.json')
const tempPath = `${targetPath}.tmp`
const backupPath = `${targetPath}.bak`

async function readJson(path, { optional = false } = {}){
    try {
        return JSON.parse(await readFile(path, 'utf8'))
    } catch(error){
        if(optional && error?.code === 'ENOENT'){
            return {}
        }

        throw error
    }
}

function mergeReleaseManagedConfig(source, local){
    if(source == null || typeof source !== 'object' || Array.isArray(source)){
        throw new Error('config/app.config.json은 JSON object여야 합니다.')
    }

    if(local == null || typeof local !== 'object' || Array.isArray(local)){
        throw new Error('로컬 app.config.json은 JSON object여야 합니다.')
    }

    return {
        ...local,
        ...source
    }
}

async function writeJsonAtomically(path, value){
    await mkdir(dirname(path), { recursive: true })
    await rm(tempPath, { force: true })
    await writeFile(tempPath, `${JSON.stringify(value, null, 4)}\n`, 'utf8')
    await rm(backupPath, { force: true })

    try {
        await rename(path, backupPath)
    } catch(error){
        if(error?.code !== 'ENOENT'){
            throw error
        }
    }

    try {
        await rename(tempPath, path)
        await rm(backupPath, { force: true })
    } catch(error){
        try {
            await rename(backupPath, path)
        } catch {
            // The original error below is more actionable.
        }

        throw error
    }
}

const source = await readJson(sourcePath)
const local = await readJson(targetPath, { optional: true })
const merged = mergeReleaseManagedConfig(source, local)

if(JSON.stringify(merged) === JSON.stringify(local)){
    console.log(`Local app config is already current: ${targetPath}`)
} else {
    await writeJsonAtomically(targetPath, merged)
    console.log(`Synced local app config: ${targetPath}`)
}
