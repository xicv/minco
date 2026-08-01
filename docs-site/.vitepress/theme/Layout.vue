<script setup lang="ts">
import { computed } from 'vue'
import { useData, withBase } from 'vitepress'
import DefaultTheme from 'vitepress/theme'

const { Layout } = DefaultTheme
const { page } = useData()
const isNext = computed(() => page.value.relativePath.startsWith('next/'))
const stableHref = withBase('/0.5.0/')
</script>

<template>
  <Layout>
    <template #doc-before>
      <div v-if="isNext" class="version-banner next-warning" role="note">
        <span>
          <strong>Unreleased documentation.</strong>
          These pages describe current development source and may change before release.
        </span>
        <a :href="stableHref">Use stable 0.5.0</a>
      </div>
    </template>
  </Layout>
</template>
