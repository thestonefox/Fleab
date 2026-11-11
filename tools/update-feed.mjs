import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign, verify } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export const targets = ['windows-x64', 'windows-arm64', 'linux-x64', 'linux-arm64', 'macos-x64', 'macos-arm64'];
export const keyId = 'fleab-updates-v1';
const publicBytes = key => Buffer.from(createPublicKey(key).export({ format: 'jwk' }).x, 'base64url');

export function generateKeys(privatePath, publicPath) {
    if (fs.existsSync(privatePath) || fs.existsSync(publicPath)) throw new Error('Refusing to overwrite an existing update signing key');
    const keys = generateKeyPairSync('ed25519');
    fs.mkdirSync(path.dirname(privatePath), { recursive: true, mode: 0o700 });
    fs.writeFileSync(privatePath, keys.privateKey.export({ type: 'pkcs8', format: 'pem' }), { flag: 'wx', mode: 0o600 });
    const jwk = keys.publicKey.export({ format: 'jwk' });
    fs.writeFileSync(publicPath, JSON.stringify({ [keyId]: Buffer.from(jwk.x, 'base64url').toString('base64') }, null, 2) + '\n', { flag: 'wx' });
}

export function signedFeed(directory, version, privatePem, keys) {
    if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) throw new Error('A stable version is required');
    const privateKey = createPrivateKey(privatePem);
    if (privateKey.asymmetricKeyType !== 'ed25519' || publicBytes(privateKey).toString('base64') !== keys[keyId]) {
        throw new Error('Signing key does not match the public key shipped in the app');
    }
    const assets = {};
    for (const target of targets) {
        const filename = `Fleab-${version}-update-${target}.tar.gz`;
        const metadata = JSON.parse(fs.readFileSync(path.join(directory, filename + '.json'), 'utf8'));
        const bytes = fs.readFileSync(path.join(directory, filename));
        const sha256 = createHash('sha256').update(bytes).digest('hex');
        if (metadata.filename !== filename || metadata.size !== bytes.length || metadata.sha256 !== sha256
            || !/^[a-f0-9]{64}$/.test(metadata.executable_sha256)
            || !Number.isSafeInteger(metadata.unpacked_size) || metadata.unpacked_size < 1) {
            throw new Error(`Invalid update metadata for ${target}`);
        }
        assets[target] = metadata;
    }
    const payload = Buffer.from(JSON.stringify({ schema_version: 1, minimum_updater_version: 1, version, assets }));
    const signature = sign(null, payload, privateKey);
    if (!verify(null, payload, createPublicKey(privateKey), signature)) throw new Error('Signature self-check failed');
    return { schema_version: 1, key_id: keyId, payload: payload.toString('base64'), signature: signature.toString('base64') };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
    try {
        const [command, first, second] = process.argv.slice(2);
        if (command === 'keygen' && first && second) {
            generateKeys(first, second);
            console.log('Created update signing key; private material was not printed.');
        } else if (command === 'sign' && first && second) {
            const keys = JSON.parse(fs.readFileSync('deployer/update-keys.json', 'utf8'));
            if (!process.env.FLEAB_UPDATE_PRIVATE_KEY) throw new Error('FLEAB_UPDATE_PRIVATE_KEY is not configured');
            fs.writeFileSync(path.join(first, 'fleab-update.json'), JSON.stringify(signedFeed(first, second.replace(/^v/, ''), process.env.FLEAB_UPDATE_PRIVATE_KEY, keys)) + '\n');
            console.log('Verified six update packages and signed the update feed.');
        } else throw new Error('Usage: update-feed.mjs keygen PRIVATE_PATH PUBLIC_PATH | sign DIRECTORY VERSION');
    } catch (error) { console.error(error.message); process.exitCode = 1; }
}
