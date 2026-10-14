/**
 * WhatsApp Web Protobuf Extractor for Rose-Baileys
 * 
 * Fetches the latest WhatsApp Web JavaScript bundles directly from web.whatsapp.com,
 * locates internalSpec Protobuf definitions via Acorn AST analysis, and generates
 * the complete proto3 syntax WAProto.proto specification.
 */

const fs = require('fs/promises');
const path = require('path');

let acorn, walk;
try {
  acorn = require('acorn');
  walk = require('acorn-walk');
} catch (e) {
  try {
    const fallbackPath = path.resolve(__dirname, '../../Baileys-custom/node_modules');
    acorn = require(require.resolve('acorn', { paths: [fallbackPath] }));
    walk = require(require.resolve('acorn-walk', { paths: [fallbackPath] }));
  } catch (err) {
    console.error('Failed to load acorn / acorn-walk. Please install them or link node_modules.');
    throw err;
  }
}

let whatsAppVersion = 'latest';

async function httpGet(url, options = {}) {
  const res = await fetch(url, options);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status} ${res.statusText} fetching ${url}`);
  }
  return res.text();
}

const addPrefix = (lines, prefix) => lines.map((line) => prefix + line);

const extractAllExpressions = (node) => {
  const expressions = [node];
  const exp = node.expression;
  if (exp) {
    expressions.push(exp);
  }
  if (node?.expression?.arguments?.length) {
    for (const arg of node?.expression?.arguments) {
      if (arg?.body?.body?.length) {
        for (const exp of arg?.body.body) {
          expressions.push(...extractAllExpressions(exp));
        }
      }
    }
  }
  if (node?.body?.body?.length) {
    for (const exp of node?.body?.body) {
      if (exp.expression) {
        expressions.push(...extractAllExpressions(exp.expression));
      }
    }
  }
  if (node.expression?.expressions?.length) {
    for (const exp of node.expression?.expressions) {
      expressions.push(...extractAllExpressions(exp));
    }
  }
  return expressions;
};

async function findAppModules() {
  const ua = {
    headers: {
      'User-Agent':
        'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36',
      'Sec-Fetch-Dest': 'script',
      'Sec-Fetch-Mode': 'no-cors',
      'Sec-Fetch-Site': 'same-origin',
      Referer: 'https://web.whatsapp.com/',
      Accept: '*/*',
      'Accept-Language': 'en-US,en;q=0.9',
    },
  };
  const baseURL = 'https://web.whatsapp.com';
  console.log(`[WAProto-Pull] Fetching service worker from ${baseURL}/sw.js...`);
  const serviceworker = await httpGet(`${baseURL}/sw.js`, ua);

  const versionMatch = serviceworker.match(/\\?"(?:client_revision|__spin_r)\\?":\s*(\d+)/);
  const version = versionMatch?.[1] || 'unknown';
  console.log(`[WAProto-Pull] Current WhatsApp Web client revision: 2.3000.${version}`);

  const waVersion = `2.3000.${version}`;
  whatsAppVersion = waVersion;

  let bootstrapQRURL = '';
  const clearString = serviceworker.replaceAll('/*BTDS*/', '');
  const urlMatches = [
    ...clearString.matchAll(/importScripts\((?:[^\)]*createScriptURL\()?\s*["']([^"']+)["']/g),
  ];
  let rawUrl = urlMatches[0]?.[1];
  if (!rawUrl) {
    const cdnMatch = clearString.match(/https?:\\?\/\\?\/(?:static\.whatsapp\.net|web\.whatsapp\.com)\\?\/rsrc\.php\\?\/[^"'\s)]+\.js/);
    if (cdnMatch) {
      rawUrl = cdnMatch[0];
    }
  }
  if (!rawUrl) {
    throw new Error('Unable to find bootstrap script URL in sw.js');
  }
  bootstrapQRURL = new URL(rawUrl.replaceAll('\\', ''), baseURL).href;

  console.log(`[WAProto-Pull] Found bootstrap script URL: ${bootstrapQRURL}`);
  console.log(`[WAProto-Pull] Downloading JS bundle...`);
  const qrData = await httpGet(bootstrapQRURL, ua);

  // This one list of types is so long that it's split into two JavaScript declarations.
  // The module finder below can't handle it, so patch it manually here.
  const patchedQrData = qrData.replaceAll(
    'LimitSharing$Trigger',
    'LimitSharing$TriggerType'
  );

  console.log(`[WAProto-Pull] Parsing AST with Acorn...`);
  const qrModules = acorn.parse(patchedQrData, { ecmaVersion: 'latest' }).body;

  const result = qrModules.filter((m) => {
    const expressions = extractAllExpressions(m);
    return expressions?.find(
      (e) => e?.left?.property?.name === 'internalSpec'
    );
  });
  console.log(`[WAProto-Pull] Found ${result.length} internalSpec modules in bundle`);
  return result;
}

async function extractProto(outputPath) {
  const unspecName = (name) =>
    name.endsWith('Spec') ? name.slice(0, -4) : name;
  const unnestName = (name) => name.split('$').slice(-1)[0];
  const getNesting = (name) => name.split('$').slice(0, -1).join('$');
  const makeRenameFunc = () => (name) => unspecName(name);

  const modules = await findAppModules();

  // Find aliases of cross references between the wanted modules
  const modulesInfo = {};
  const moduleIndentationMap = {};
  modules.forEach((module) => {
    const moduleName = module.expression.arguments[0].value;
    modulesInfo[moduleName] = { crossRefs: [] };
    walk.simple(module, {
      AssignmentExpression(node) {
        if (
          node &&
          node?.right?.type === 'CallExpression' &&
          node?.right?.arguments?.length === 1 &&
          node?.right?.arguments[0].type !== 'ObjectExpression'
        ) {
          modulesInfo[moduleName].crossRefs.push({
            alias: node.left.name,
            module: node.right.arguments[0].value,
          });
        }
      },
    });
  });

  // Find all identifiers and, for enums, their array of values
  for (const mod of modules) {
    const modInfo = modulesInfo[mod.expression.arguments[0].value];
    const rename = makeRenameFunc();

    const assignments = [];
    walk.simple(mod, {
      AssignmentExpression(node) {
        const left = node.left;
        if (
          left.property?.name &&
          left.property?.name !== 'internalSpec' &&
          left.property?.name !== 'internalDefaults' &&
          left.property?.name !== 'name'
        ) {
          assignments.push(left);
        }
      },
    });

    const makeBlankIdent = (a) => {
      const key = rename(a?.property?.name);
      const indentation = getNesting(key);
      const value = { name: key };

      moduleIndentationMap[key] = moduleIndentationMap[key] || {};
      moduleIndentationMap[key].indentation = indentation;

      if (indentation.length) {
        moduleIndentationMap[indentation] =
          moduleIndentationMap[indentation] || {};
        moduleIndentationMap[indentation].members =
          moduleIndentationMap[indentation].members || new Set();
        moduleIndentationMap[indentation].members.add(key);
      }

      return [key, value];
    };

    modInfo.identifiers = Object.fromEntries(
      assignments.map(makeBlankIdent).reverse()
    );
    const enumAliases = {};
    walk.ancestor(mod, {
      Property(node, anc) {
        const fatherNode = anc[anc.length - 3];
        const fatherFather = anc[anc.length - 4];
        if (
          fatherNode?.type === 'AssignmentExpression' &&
          fatherNode?.left?.property?.name === 'internalSpec' &&
          fatherNode?.right?.properties?.length
        ) {
          const values = fatherNode?.right?.properties.map((p) => ({
            name: p.key.name,
            id: p.value.value,
          }));
          const nameAlias = fatherNode?.left?.name;
          enumAliases[nameAlias] = values;
        } else if (node?.key && node?.key?.name && fatherNode.arguments?.length > 0) {
          const values = fatherNode.arguments?.[0]?.properties?.map((p) => ({
            name: p.key.name,
            id: p.value.value,
          }));
          const nameAlias = fatherFather?.left?.name || fatherFather.id?.name;
          enumAliases[nameAlias] = values;
        }
      },
    });
    walk.simple(mod, {
      AssignmentExpression(node) {
        if (
          node.left.type === 'MemberExpression' &&
          modInfo.identifiers?.[rename(node.left.property.name)]
        ) {
          const ident = modInfo.identifiers[rename(node.left.property.name)];
          ident.alias = node.right.name;
          ident.enumValues = enumAliases[ident.alias];
        }
      },
    });
  }

  // Find the contents for all protobuf messages
  for (const mod of modules) {
    const modInfo = modulesInfo[mod.expression.arguments[0].value];
    const rename = makeRenameFunc();
    const findByAliasInIdentifier = (obj, alias) => {
      return Object.values(obj).find((item) => item.alias === alias);
    };

    walk.simple(mod, {
      AssignmentExpression(node) {
        if (
          node.left.type === 'MemberExpression' &&
          node.left.property.name === 'internalSpec' &&
          node.right.type === 'ObjectExpression'
        ) {
          const targetIdent = Object.values(modInfo.identifiers).find(
            (v) => v.alias === node.left.object.name
          );
          if (!targetIdent) {
            return;
          }

          const constraints = [];
          let members = [];
          for (const p of node.right.properties) {
            p.key.name = p.key.type === 'Identifier' ? p.key.name : p.key.value;
            const arr =
              p.key.name.substr(0, 2) === '__' ? constraints : members;
            arr.push(p);
          }

          members = members.map(({ key: { name }, value: { elements } }) => {
            let type;
            const flags = [];
            const unwrapBinaryOr = (n) =>
              n.type === 'BinaryExpression' && n.operator === '|'
                ? [].concat(unwrapBinaryOr(n.left), unwrapBinaryOr(n.right))
                : [n];

            unwrapBinaryOr(elements[1]).forEach((m) => {
              if (
                m.type === 'MemberExpression' &&
                m.object.type === 'MemberExpression'
              ) {
                if (m.object.property.name === 'TYPES') {
                  type = m.property.name.toLowerCase();
                  if (type === 'map') {
                    let typeStr = 'map<';
                    if (elements[2]?.type === 'ArrayExpression') {
                      const subElements = elements[2].elements;
                      subElements.forEach((element, index) => {
                        if (element?.property?.name) {
                          typeStr += element?.property?.name?.toLowerCase();
                        } else {
                          const ref = findByAliasInIdentifier(modInfo.identifiers, element.name);
                          typeStr += ref?.name || 'string';
                        }
                        if (index < subElements.length - 1) {
                          typeStr += ', ';
                        }
                      });
                      typeStr += '>';
                      type = typeStr;
                    }
                  }
                } else if (m.object.property.name === 'FLAGS') {
                  flags.push(m.property.name.toLowerCase());
                }
              }
            });

            if (type === 'message' || type === 'enum') {
              if (elements[2].type === 'Identifier') {
                type = Object.values(modInfo.identifiers).find(
                  (v) => v.alias === elements[2].name
                )?.name;
              } else if (elements[2].type === 'MemberExpression') {
                let crossRef = modInfo.crossRefs.find(
                  (r) =>
                    r.alias === elements[2]?.object?.name ||
                    r.alias === elements[2]?.object?.left?.name ||
                    r.alias === elements[2]?.object?.callee?.name
                );
                if (
                  elements[1]?.property?.name === 'ENUM' &&
                  elements[2]?.property?.name?.includes('Type')
                ) {
                  type = rename(elements[2]?.property?.name);
                } else if (elements[2]?.property?.name.includes('Spec')) {
                  type = rename(elements[2].property.name);
                } else if (
                  crossRef &&
                  crossRef.module !== '$InternalEnum' &&
                  modulesInfo[crossRef.module]?.identifiers[
                    rename(elements[2].property.name)
                  ]
                ) {
                  type = rename(elements[2].property.name);
                }
              }
            }

            return { name, id: elements[0].value, type: type || 'string', flags };
          });

          // resolve constraints for members
          constraints.forEach((c) => {
            if (
              c.key.name === '__oneofs__' &&
              c.value.type === 'ObjectExpression'
            ) {
              const newOneOfs = c.value.properties.map((p) => ({
                name: p.key.name,
                type: '__oneof__',
                members: p.value.elements.map((e) => {
                  const idx = members.findIndex((m) => m.name === e.value);
                  const member = members[idx];
                  if (idx !== -1) members.splice(idx, 1);
                  return member;
                }).filter(Boolean),
              }));
              members.push(...newOneOfs);
            }
          });

          targetIdent.members = members;
        }
      },
    });
  }

  const decodedProtoMap = {};
  const spaceIndent = ' '.repeat(4);
  for (const mod of modules) {
    const modInfo = modulesInfo[mod.expression.arguments[0].value];
    const identifiers = Object.values(modInfo?.identifiers || {});

    const stringifyEnum = (ident, overrideName = null) =>
      [].concat(
        [`enum ${overrideName || ident.displayName || ident.name} {`],
        addPrefix(
          (ident.enumValues || []).map((v) => `${v.name} = ${v.id};`),
          spaceIndent
        ),
        ['}']
      );

    const stringifyMessageSpecMember = (
      info,
      completeFlags,
      parentName = undefined
    ) => {
      if (info.type === '__oneof__') {
        return [].concat(
          [`oneof ${info.name} {`],
          addPrefix(
            [].concat(
              ...info.members.map((m) => stringifyMessageSpecMember(m, false))
            ),
            spaceIndent
          ),
          ['}']
        );
      } else {
        if (info.flags.includes('packed')) {
          info.flags.splice(info.flags.indexOf('packed'));
          info.packed = ' [packed=true]';
        }
        if (info.flags.includes('required')) {
          info.flags = info.flags.map((f) => (f === 'required' ? 'optional' : f));
        }
        if (completeFlags && info.flags.length === 0 && !info.type.includes('map')) {
          info.flags.push('optional');
        }

        const ret = [];
        const indentation = moduleIndentationMap[info.type]?.indentation;
        let typeName = unnestName(info.type);
        if (indentation !== parentName && indentation) {
          typeName = `${indentation.replaceAll('$', '.')}.${typeName}`;
        }

        ret.push(
          `${info.flags.join(' ') + (info.flags.length === 0 ? '' : ' ')}${typeName} ${info.name} = ${info.id}${info.packed || ''};`
        );
        return ret;
      }
    };

    const stringifyMessageSpec = (ident) => {
      const members = moduleIndentationMap[ident.name]?.members;
      const result = [];
      result.push(
        `message ${ident.displayName || ident.name} {`,
        ...addPrefix(
          [].concat(
            ...(ident.members || []).map((m) =>
              stringifyMessageSpecMember(m, true, ident.name)
            )
          ),
          spaceIndent
        )
      );

      if (members?.size) {
        const sortedMembers = Array.from(members).sort();
        for (const memberName of sortedMembers) {
          let entity = modInfo.identifiers[memberName];
          if (entity) {
            const displayName = entity.name.slice(ident.name.length + 1);
            entity = { ...entity, displayName };
            result.push(...addPrefix(getEntity(entity), spaceIndent));
          }
        }
      }

      result.push('}');
      result.push('');
      return result;
    };

    const getEntity = (v) => {
      if (v.members) {
        return stringifyMessageSpec(v);
      } else if (v.enumValues?.length) {
        return stringifyEnum(v);
      } else {
        return ['// Unknown entity ' + v.name];
      }
    };

    const stringifyEntity = (v) => {
      return {
        content: getEntity(v).join('\n'),
        name: v.name,
      };
    };

    for (const value of identifiers) {
      const { name, content } = stringifyEntity(value);
      if (!moduleIndentationMap[name]?.indentation?.length) {
        decodedProtoMap[name] = content;
      }
    }
  }

  const decodedProto = Object.keys(decodedProtoMap).sort();
  const sortedStr = decodedProto.map((d) => decodedProtoMap[d]).join('\n');

  let decodedProtoStr = `syntax = "proto2";\npackage proto;\n\n/// WhatsApp Version: ${whatsAppVersion}\n\n${sortedStr}`.replaceAll(/\brequired\b/g, 'optional');
  if (!decodedProtoStr.includes('message LIDMigrationMappingSyncPayload')) {
    const lidHelper = `\nmessage LIDMigrationMapping {\n    optional uint64 pn = 1;\n    optional uint64 assignedLid = 2;\n    optional uint64 latestLid = 3;\n    optional uint64 chatDbMigrationTimestamp = 4;\n}\n\nmessage LIDMigrationMappingSyncPayload {\n    repeated LIDMigrationMapping pnToLidMappings = 1;\n    optional uint64 chatDbMigrationTimestamp = 2;\n}\n`;
    if (decodedProtoStr.includes('message LIDMigrationMappingSyncMessage {')) {
      decodedProtoStr = decodedProtoStr.replace(
        /message LIDMigrationMappingSyncMessage\s*\{[\s\S]*?\}/,
        (match) => match + '\n' + lidHelper
      );
    } else {
      decodedProtoStr += '\n' + lidHelper;
    }
  }

  await fs.writeFile(outputPath, decodedProtoStr, 'utf8');
  console.log(`[WAProto-Pull] Successfully extracted fresh schema (${decodedProto.length} root entities) to ${outputPath}`);
}

(async () => {
  const target = process.argv[2] || path.resolve(__dirname, '../crates/baileys-proto/proto/WAProto.proto');
  try {
    await extractProto(target);
  } catch (err) {
    console.error('[WAProto-Pull] Error extracting WAProto:', err);
    process.exit(1);
  }
})();
