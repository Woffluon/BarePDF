import { defineCollection, z } from 'astro:content';

const docsCollection = defineCollection({
  type: 'content',
  schema: z.object({
    title: z.string(),
    description: z.string(),
    category: z.enum(['user', 'developer']),
    order: z.number().default(0),
    updatedAt: z.string().optional(),
  }),
});

export const collections = {
  docs: docsCollection,
};
