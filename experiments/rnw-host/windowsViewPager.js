function mapPagerScrollOffset(offset, width, childCount) {
  if (!Number.isFinite(offset) || !Number.isFinite(width) || width <= 0) {
    return {position: 0, offset: 0};
  }
  const lastPage = Math.max(0, childCount - 1);
  const positionValue = Math.max(0, Math.min(offset / width, lastPage));
  const position = Math.floor(positionValue);
  return {position, offset: positionValue - position};
}

function isPagerScrollMapping(mapping) {
  const nativeEvent = mapping?.[0]?.nativeEvent;
  return nativeEvent != null &&
    Object.prototype.hasOwnProperty.call(nativeEvent, 'position') &&
    Object.prototype.hasOwnProperty.call(nativeEvent, 'offset');
}

function createLegacyAnimated(animated) {
  if (!animated || typeof animated.event !== 'function') return animated;
  const legacyAnimated = {...animated};
  const nativeEvent = animated.event.bind(animated);
  legacyAnimated.event = (mapping, config) => {
    if (config?.useNativeDriver) {
      return nativeEvent(mapping, {...config, useNativeDriver: false});
    }
    return nativeEvent(mapping, config);
  };
  // RNW's Fabric native animation node path is not ABI-compatible with the
  // legacy phone bundles. In particular, reading a legacy animated value can
  // crash inside ValueAnimatedNode::RawValue. Keep these plugin animations
  // smooth through the JS driver; this preserves progress/state callbacks and
  // avoids creating incompatible native nodes.
  for (const name of ['timing', 'spring', 'decay']) {
    if (typeof animated[name] === 'function') {
      const original = animated[name].bind(animated);
      legacyAnimated[name] = (value, config = {}) => original(value, {...config, useNativeDriver: false});
    }
  }
  return legacyAnimated;
}

function createWindowsViewPager(React, RN) {
  const ViewPagerAndroid = React.forwardRef((props, forwardedRef) => {
    const {
      children,
      initialPage = 0,
      onPageScroll,
      onPageSelected,
      onLayout,
      onScroll,
      ...scrollProps
    } = props;
    const scrollViewRef = React.useRef(null);
    const widthRef = React.useRef(RN.Dimensions?.get?.('window')?.width || 0);
    const laidOutRef = React.useRef(false);
    const pageCount = React.Children.count(children);

    const scrollToPage = (page, animated) => {
      const width = widthRef.current;
      if (!scrollViewRef.current || width <= 0 || !Number.isFinite(Number(page))) return;
      const lastPage = Math.max(0, pageCount - 1);
      const selectedPage = Math.max(0, Math.min(Math.round(Number(page)), lastPage));
      scrollViewRef.current.scrollTo({x: selectedPage * width, y: 0, animated});
    };

    const handle = {
      getNode() {return handle;},
      setPage(page) {scrollToPage(page, true);},
      setPageWithoutAnimation(page) {scrollToPage(page, false);},
      setNativeProps(nativeProps) {scrollViewRef.current?.setNativeProps?.(nativeProps);},
    };
    React.useImperativeHandle(forwardedRef, () => handle, []);

    const handleLayout = event => {
      const measuredWidth = event?.nativeEvent?.layout?.width;
      if (Number.isFinite(measuredWidth) && measuredWidth > 0) widthRef.current = measuredWidth;
      if (!laidOutRef.current) {
        laidOutRef.current = true;
        scrollToPage(initialPage, false);
      }
      onLayout?.(event);
    };

    const pageFromEvent = event => mapPagerScrollOffset(
      Number(event?.nativeEvent?.contentOffset?.x || 0), widthRef.current, pageCount,
    );
    const handleScroll = event => {
      const page = pageFromEvent(event);
      onPageScroll?.({nativeEvent: page});
      onScroll?.(event);
    };
    const handleMomentumEnd = event => {
      const page = pageFromEvent(event);
      onPageSelected?.({nativeEvent: {position: Math.round(page.position + page.offset)}});
    };

    return React.createElement(RN.ScrollView, {
      ...scrollProps,
      ref: scrollViewRef,
      horizontal: true,
      pagingEnabled: true,
      showsHorizontalScrollIndicator: false,
      scrollEventThrottle: 16,
      onLayout: handleLayout,
      onScroll: handleScroll,
      onMomentumScrollEnd: handleMomentumEnd,
    }, children);
  });
  ViewPagerAndroid.displayName = 'WindowsCompatViewPagerAndroid';
  return ViewPagerAndroid;
}

module.exports = {createLegacyAnimated, createWindowsViewPager, isPagerScrollMapping, mapPagerScrollOffset};
