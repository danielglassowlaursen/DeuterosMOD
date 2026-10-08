using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Deuteros.Code.Objects
{
    public class ProductionItem
    {
        public Item Product { get; set; }
        public int Production_Value { get; set; }
        public int Object_Multiplier { get; set; }
        public int Production_Complete { get; set; }
        public bool AOCRepeat { get; set; }
        public bool AOCOneTime { get; set; }
        public bool Active { get; set; }

        public ProductionItem(Item product) 
        {
            Product = product;
            Object_Multiplier = product.Research.ResearchMultiplier;
            Production_Complete = 1;
        }

        public bool Complete 
        { 
            get 
            {
                return Production_Complete == 4;
            } 
        }
    }
}