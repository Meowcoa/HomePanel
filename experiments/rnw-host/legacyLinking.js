function createLegacyLinking(linking) {
  if (!linking) return linking;
  const legacyLinking = Object.create(Object.getPrototypeOf(linking));
  for (const [key, descriptor] of Object.entries(Object.getOwnPropertyDescriptors(linking))) {
    Object.defineProperty(legacyLinking, key, {...descriptor, configurable: true});
  }

  const addEventListener = typeof linking.addEventListener === 'function'
    ? linking.addEventListener.bind(linking) : null;
  const removeLegacyListener = typeof linking.removeEventListener === 'function'
    ? linking.removeEventListener.bind(linking) : null;
  const subscriptions = new Map();

  function forget(type, listener, subscription) {
    const listeners = subscriptions.get(type);
    const remaining = listeners?.get(listener)?.filter(item => item !== subscription) || [];
    if (remaining.length) listeners.set(listener, remaining);
    else listeners?.delete(listener);
    if (listeners?.size === 0) subscriptions.delete(type);
  }

  Object.defineProperties(legacyLinking, {
    addEventListener: {configurable: true, value(type, listener) {
      if (!addEventListener) return undefined;
      const nativeSubscription = addEventListener(type, listener);
      let active = true;
      const subscription = {remove() {
        if (!active) return;
        active = false;
        forget(type, listener, subscription);
        if (typeof nativeSubscription?.remove === 'function') nativeSubscription.remove();
        else removeLegacyListener?.(type, listener);
      }};
      let listeners = subscriptions.get(type);
      if (!listeners) {listeners = new Map(); subscriptions.set(type, listeners);}
      const matches = listeners.get(listener) || [];
      matches.push(subscription);
      listeners.set(listener, matches);
      return subscription;
    }},
    removeEventListener: {configurable: true, value(type, listener) {
      const matches = [...(subscriptions.get(type)?.get(listener) || [])];
      if (matches.length) {
        matches.forEach(subscription => subscription.remove());
      } else {
        removeLegacyListener?.(type, listener);
      }
    }},
  });
  return legacyLinking;
}

module.exports = {createLegacyLinking};
