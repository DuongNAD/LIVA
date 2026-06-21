const { builtinModules } = require('module');

try {
  const Resolver = require('jest-resolve').default;
  if (Resolver && Resolver.prototype) {
    const originalIsCoreModule = Resolver.prototype.isCoreModule;
    Resolver.prototype.isCoreModule = function (moduleName) {
      if (typeof moduleName === 'string' && (moduleName.startsWith('node:') || moduleName === 'sqlite')) {
        return true;
      }
      return originalIsCoreModule.call(this, moduleName);
    };
  }
} catch (e) {
  console.error('[RESOLVER] Failed to patch jest-resolve:', e);
}

module.exports = function (request, options) {
  let resolvedRequest = request;
  if (request.startsWith('node:')) {
    resolvedRequest = request.slice(5);
  }
  const base = resolvedRequest.split('/')[0];
  if (
    base === 'sqlite' ||
    builtinModules.includes(base) ||
    builtinModules.includes(resolvedRequest)
  ) {
    return resolvedRequest;
  }
  return options.defaultResolver(request, options);
};
