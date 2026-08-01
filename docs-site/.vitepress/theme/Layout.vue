<script setup lang="ts">
import { computed } from 'vue'
import { useData, withBase } from 'vitepress'
import DefaultTheme from 'vitepress/theme'
import release from '../../release.json'

const { Layout } = DefaultTheme
const { page } = useData()
const isNext = computed(() => page.value.relativePath.startsWith('next/'))
const isCandidate = computed(
  () =>
    release.state === 'candidate' &&
    page.value.relativePath.startsWith(`${release.workspace}/`)
)
const stableHref = withBase(`/${release.stable}/`)
</script>

<template>
  <Layout>
    <template #doc-before>
      <div v-if="isNext" class="version-banner next-warning" role="note">
        <span>
          <strong>Unreleased documentation.</strong>
          These pages describe current development source and may change before release.
        </span>
        <a :href="stableHref">Use stable {{ release.stable }}</a>
      </div>
      <div v-if="isCandidate" class="version-banner next-warning" role="note">
        <span>
          <strong>Release candidate documentation.</strong>
          This version is qualified source, not yet an independently verified registry release.
        </span>
        <a :href="stableHref">Use stable {{ release.stable }}</a>
      </div>
    </template>
  </Layout>
</template>
